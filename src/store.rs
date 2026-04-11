use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use crate::config::AppConfig;
use crate::error::{Result, SlotdError};
use crate::job::{JobRecord, JobState, NodeInfo, SubmitRequest};
use crate::sbatch::{
    default_batch_output_pattern, expand_output_pattern, parse_array_spec, resolve_log_path,
};

const MIGRATION_SQL: &str = include_str!("../migrations/0001_init.sql");

pub struct Store {
    conn: Connection,
    config: AppConfig,
}

impl Store {
    pub fn open(config: AppConfig) -> Result<Self> {
        config.ensure_dirs()?;
        let conn = Connection::open(&config.db_path)?;
        conn.execute_batch(MIGRATION_SQL)?;
        let store = Self { conn, config };
        store.ensure_compat_schema()?;
        Ok(store)
    }

    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    pub fn create_job(&self, request: SubmitRequest) -> Result<i64> {
        if request.array_spec.is_some() {
            return self.create_array_jobs(request);
        }
        self.create_job_entry(
            &request,
            None,
            None,
            None,
            None,
            request
                .name
                .clone()
                .unwrap_or_else(|| default_name(&request.script_name)),
        )
    }

    fn create_array_jobs(&self, request: SubmitRequest) -> Result<i64> {
        let spec = parse_array_spec(
            request
                .array_spec
                .as_deref()
                .ok_or_else(|| SlotdError::from("missing array specification"))?,
        )?;
        let base_name = request
            .name
            .clone()
            .unwrap_or_else(|| default_name(&request.script_name));
        let task_count = spec.task_ids.len() as u32;
        let mut array_job_id = None;

        for task_id in spec.task_ids {
            let job_id = self.create_job_entry(
                &request,
                array_job_id,
                Some(task_id),
                Some(task_count),
                spec.limit,
                base_name.clone(),
            )?;
            if array_job_id.is_none() {
                array_job_id = Some(job_id);
                self.conn
                    .execute("UPDATE jobs SET array_job_id = ?1 WHERE id = ?1", [job_id])?;
            }
        }

        array_job_id.ok_or_else(|| SlotdError::from("array specification produced no jobs"))
    }

    fn create_job_entry(
        &self,
        request: &SubmitRequest,
        array_job_id: Option<i64>,
        array_task_id: Option<i32>,
        array_task_count: Option<u32>,
        array_task_limit: Option<u32>,
        resolved_name: String,
    ) -> Result<i64> {
        let submit_time = now_ts();
        let user_name = request.user_name.clone();
        self.conn.execute(
            "INSERT INTO jobs (
                parent_job_id, step_id, held, priority, name, user_name, state, partition, command, cwd, requested_cpus, requested_memory_mb,
                requested_tasks, requested_gpus, allocation_only, dependency,
                array_job_id, array_task_id, array_task_count, array_task_limit, submit_time,
                state_reason, time_limit_secs
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)",
            params![
                Option::<i64>::None,
                Option::<i64>::None,
                false,
                0i32,
                resolved_name,
                user_name,
                JobState::Pending.as_str(),
                &request.partition,
                request
                    .command_override
                    .clone()
                    .unwrap_or_else(|| script_command(&request.script_name)),
                &request.cwd,
                request.requested_cpus,
                request.requested_memory_mb,
                request.requested_tasks,
                request.requested_gpus,
                request.allocation_only,
                request.dependency.as_deref(),
                array_job_id,
                array_task_id,
                array_task_count.map(|value| value as i64),
                array_task_limit.map(|value| value as i64),
                submit_time,
                "Priority",
                request.time_limit_secs.map(|value| value as i64),
            ],
        )?;

        let job_id = self.conn.last_insert_rowid();
        let job_dir = self.config.jobs_dir.join(job_id.to_string());
        fs::create_dir_all(&job_dir)?;

        let script_path = job_dir.join("script.sh");
        fs::write(&script_path, &request.script_body)?;

        let default_stdout = expand_output_pattern(
            default_output_pattern(array_task_id),
            job_id,
            &resolved_name,
            &user_name,
            &self.config.hostname,
            array_job_id,
            array_task_id,
        );
        let stdout_path = request
            .stdout_path
            .as_deref()
            .map(|path| {
                expand_output_pattern(
                    path,
                    job_id,
                    &resolved_name,
                    &user_name,
                    &self.config.hostname,
                    array_job_id,
                    array_task_id,
                )
            })
            .unwrap_or(default_stdout);
        let stdout_path = PathBuf::from(resolve_log_path(&request.cwd, &stdout_path));
        let stderr_path = request
            .stderr_path
            .as_deref()
            .map(|path| {
                expand_output_pattern(
                    path,
                    job_id,
                    &resolved_name,
                    &user_name,
                    &self.config.hostname,
                    array_job_id,
                    array_task_id,
                )
            })
            .map(|path| PathBuf::from(resolve_log_path(&request.cwd, &path)))
            .unwrap_or_else(|| stdout_path.clone());
        ensure_parent_dir(&stdout_path)?;
        ensure_parent_dir(&stderr_path)?;

        self.conn.execute(
            "UPDATE jobs
             SET script_path = ?1, stdout_path = ?2, stderr_path = ?3
             WHERE id = ?4",
            params![
                path_string(&script_path),
                path_string(&stdout_path),
                path_string(&stderr_path),
                job_id
            ],
        )?;

        Ok(job_id)
    }

    pub fn create_step(
        &self,
        parent_job_id: i64,
        name: String,
        command: String,
        cwd: String,
        user_name: String,
    ) -> Result<i64> {
        let parent = self
            .get_job(parent_job_id)?
            .ok_or_else(|| SlotdError::from(format!("parent job {parent_job_id} not found")))?;
        let next_step_id = self.next_step_id(parent_job_id)?;
        let submit_time = now_ts();
        self.conn.execute(
            "INSERT INTO jobs (
                parent_job_id, step_id, held, priority, name, user_name, state, partition, command, cwd,
                requested_cpus, requested_memory_mb, requested_tasks, requested_gpus, allocation_only,
                dependency, array_job_id, array_task_id, array_task_count, array_task_limit,
                submit_time, start_time, state_reason, time_limit_secs, script_path, stdout_path, stderr_path
            ) VALUES (?1, ?2, 0, 0, ?3, ?4, 'RUNNING', ?5, ?6, ?7, 0, 0, 0, 0, 0, NULL, NULL, NULL, NULL, NULL, ?8, ?9, '', NULL, '', '', '')",
            params![
                parent_job_id,
                next_step_id as i64,
                name,
                user_name,
                parent.partition,
                command,
                cwd,
                submit_time,
                submit_time,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_steps(&self, parent_job_id: i64) -> Result<Vec<JobRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, parent_job_id, step_id, held, priority, name, user_name, state, partition, command, cwd, requested_cpus, requested_memory_mb,
                    requested_tasks, requested_gpus, allocation_only, dependency, array_job_id,
                    array_task_id, array_task_count, array_task_limit, max_rss_kb,
                    submit_time, start_time, end_time, pid, pgid, exit_code, state_reason, term_signal, time_limit_secs,
                    assigned_gpus, script_path, stdout_path, stderr_path
             FROM jobs
             WHERE parent_job_id = ?1
             ORDER BY step_id ASC, id ASC",
        )?;
        let rows = stmt.query_map([parent_job_id], map_job)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    fn next_step_id(&self, parent_job_id: i64) -> Result<u32> {
        next_step_id_query(&self.conn, parent_job_id)
    }

    pub fn list_jobs(
        &self,
        states: Option<&[JobState]>,
        ids: Option<&[i64]>,
        user_name: Option<&str>,
        partitions: Option<&[String]>,
    ) -> Result<Vec<JobRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, parent_job_id, step_id, held, priority, name, user_name, state, partition, command, cwd, requested_cpus, requested_memory_mb,
                    requested_tasks, requested_gpus, allocation_only, dependency, array_job_id,
                    array_task_id, array_task_count, array_task_limit, max_rss_kb,
                    submit_time, start_time, end_time, pid, pgid, exit_code, state_reason, term_signal, time_limit_secs,
                    assigned_gpus, script_path, stdout_path, stderr_path
             FROM jobs
             WHERE parent_job_id IS NULL"
        )?;
        let rows = stmt.query_map([], map_job)?;
        let mut jobs = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        jobs.sort_by(|a, b| b.id.cmp(&a.id));
        Ok(filter_jobs(
            jobs, states, ids, user_name, partitions, None, None,
        ))
    }

    pub fn list_accounting_jobs(
        &self,
        states: Option<&[JobState]>,
        ids: Option<&[i64]>,
        user_name: Option<&str>,
        partitions: Option<&[String]>,
        start_time: Option<i64>,
        end_time: Option<i64>,
    ) -> Result<Vec<JobRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, parent_job_id, step_id, held, priority, name, user_name, state, partition, command, cwd, requested_cpus, requested_memory_mb,
                    requested_tasks, requested_gpus, allocation_only, dependency, array_job_id,
                    array_task_id, array_task_count, array_task_limit, max_rss_kb,
                    submit_time, start_time, end_time, pid, pgid, exit_code, state_reason, term_signal, time_limit_secs,
                    assigned_gpus, script_path, stdout_path, stderr_path
             FROM jobs"
        )?;
        let rows = stmt.query_map([], map_job)?;
        let mut jobs = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        jobs.sort_by(|a, b| b.id.cmp(&a.id));
        Ok(filter_jobs(
            jobs, states, ids, user_name, partitions, start_time, end_time,
        ))
    }

    pub fn list_running_jobs(&self) -> Result<Vec<JobRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, parent_job_id, step_id, held, priority, name, user_name, state, partition, command, cwd, requested_cpus, requested_memory_mb,
                    requested_tasks, requested_gpus, allocation_only, dependency, array_job_id,
                    array_task_id, array_task_count, array_task_limit, max_rss_kb,
                    submit_time, start_time, end_time, pid, pgid, exit_code, state_reason, term_signal, time_limit_secs,
                    assigned_gpus, script_path, stdout_path, stderr_path
             FROM jobs
             WHERE state = 'RUNNING'
             ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], map_job)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn get_job(&self, job_id: i64) -> Result<Option<JobRecord>> {
        self.conn
            .query_row(
                "SELECT id, parent_job_id, step_id, held, priority, name, user_name, state, partition, command, cwd, requested_cpus, requested_memory_mb,
                        requested_tasks, requested_gpus, allocation_only, dependency, array_job_id,
                        array_task_id, array_task_count, array_task_limit, max_rss_kb,
                        submit_time, start_time, end_time, pid, pgid, exit_code, state_reason, term_signal, time_limit_secs,
                        assigned_gpus, script_path, stdout_path, stderr_path
                 FROM jobs
                 WHERE id = ?1",
                [job_id],
                map_job,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn next_pending_jobs(&self) -> Result<Vec<JobRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, parent_job_id, step_id, held, priority, name, user_name, state, partition, command, cwd, requested_cpus, requested_memory_mb,
                    requested_tasks, requested_gpus, allocation_only, dependency, array_job_id,
                    array_task_id, array_task_count, array_task_limit, max_rss_kb,
                    submit_time, start_time, end_time, pid, pgid, exit_code, state_reason, term_signal, time_limit_secs,
                    assigned_gpus, script_path, stdout_path, stderr_path
             FROM jobs
             WHERE state = 'PENDING' AND parent_job_id IS NULL
             ORDER BY id ASC
            ",
        )?;
        let rows = stmt.query_map([], map_job)?;
        let jobs = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(order_pending_jobs(jobs))
    }

    pub fn mark_running(
        &self,
        job_id: i64,
        pid: i32,
        pgid: i32,
        assigned_gpu_ids: &[u32],
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE jobs
             SET state = 'RUNNING', state_reason = '', pid = ?1, pgid = ?2, start_time = ?3, assigned_gpus = ?4
             WHERE id = ?5",
            params![pid, pgid, now_ts(), join_gpu_ids(assigned_gpu_ids), job_id],
        )?;
        Ok(())
    }

    pub fn mark_allocation_running(&self, job_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE jobs
             SET state = 'RUNNING', state_reason = '', start_time = ?1
             WHERE id = ?2",
            params![now_ts(), job_id],
        )?;
        Ok(())
    }

    pub fn adopt_allocation(&self, job_id: i64, pid: i32, pgid: i32) -> Result<()> {
        self.conn.execute(
            "UPDATE jobs
             SET pid = ?1, pgid = ?2
             WHERE id = ?3",
            params![pid, pgid, job_id],
        )?;
        Ok(())
    }

    pub fn mark_finished(
        &self,
        job_id: i64,
        state: JobState,
        exit_code: Option<i32>,
        term_signal: Option<i32>,
        state_reason: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE jobs
             SET state = ?1, exit_code = ?2, term_signal = ?3, state_reason = ?4, end_time = ?5, assigned_gpus = ''
             WHERE id = ?6",
            params![
                state.as_str(),
                exit_code,
                term_signal,
                state_reason.unwrap_or(""),
                now_ts(),
                job_id
            ],
        )?;
        Ok(())
    }

    pub fn mark_state(
        &self,
        job_id: i64,
        state: JobState,
        state_reason: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE jobs
             SET state = ?1, state_reason = ?2
             WHERE id = ?3",
            params![state.as_str(), state_reason.unwrap_or(""), job_id],
        )?;
        Ok(())
    }

    pub fn update_max_rss(&self, job_id: i64, max_rss_kb: u64) -> Result<()> {
        self.conn.execute(
            "UPDATE jobs
             SET max_rss_kb = CASE
                 WHEN max_rss_kb IS NULL OR max_rss_kb < ?1 THEN ?1
                 ELSE max_rss_kb
             END
             WHERE id = ?2",
            params![max_rss_kb as i64, job_id],
        )?;
        Ok(())
    }

    pub fn hold_job(&self, job_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE jobs SET held = 1, state_reason = 'JobHeldUser' WHERE id = ?1 AND state = 'PENDING'",
            [job_id],
        )?;
        Ok(())
    }

    pub fn release_job(&self, job_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE jobs SET held = 0, state_reason = 'Priority' WHERE id = ?1 AND state = 'PENDING'",
            [job_id],
        )?;
        Ok(())
    }

    pub fn update_job_fields(
        &self,
        job_id: i64,
        name: Option<&str>,
        partition: Option<&str>,
        time_limit_secs: Option<u64>,
        priority: Option<i32>,
    ) -> Result<()> {
        let job = self
            .get_job(job_id)?
            .ok_or_else(|| SlotdError::from(format!("job {job_id} not found")))?;

        if let Some(value) = name {
            if job.state != JobState::Pending {
                return Err(SlotdError::from("job name can only be updated while pending"));
            }
            self.conn
                .execute("UPDATE jobs SET name = ?1 WHERE id = ?2", params![value, job_id])?;
        }
        if let Some(value) = partition {
            if job.state != JobState::Pending {
                return Err(SlotdError::from("partition can only be updated while pending"));
            }
            self.conn.execute(
                "UPDATE jobs SET partition = ?1 WHERE id = ?2 AND state = 'PENDING'",
                params![value, job_id],
            )?;
        }
        if let Some(value) = time_limit_secs {
            if job.state.is_terminal() {
                return Err(SlotdError::from(
                    "time limit cannot be updated after the job has finished",
                ));
            }
            self.conn.execute(
                "UPDATE jobs SET time_limit_secs = ?1 WHERE id = ?2",
                params![value as i64, job_id],
            )?;
        }
        if let Some(value) = priority {
            if job.state != JobState::Pending {
                return Err(SlotdError::from("priority can only be updated while pending"));
            }
            self.conn.execute(
                "UPDATE jobs SET priority = ?1 WHERE id = ?2",
                params![value, job_id],
            )?;
        }
        Ok(())
    }

    pub fn running_array_tasks(&self, array_job_id: i64) -> Result<u32> {
        let count = self.conn.query_row(
            "SELECT COUNT(*) FROM jobs WHERE array_job_id = ?1 AND state = 'RUNNING'",
            [array_job_id],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(count.max(0) as u32)
    }

    pub fn has_active_job_with_name(
        &self,
        user_name: &str,
        job_name: &str,
        exclude_job_id: i64,
    ) -> Result<bool> {
        let count = self.conn.query_row(
            "SELECT COUNT(*) FROM jobs
             WHERE user_name = ?1 AND name = ?2 AND id <> ?3 AND state IN ('PENDING', 'RUNNING', 'COMPLETING')",
            params![user_name, job_name, exclude_job_id],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(count > 0)
    }

    pub fn cancel_pending_job(&self, job_id: i64) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE jobs
             SET state = 'CANCELLED', state_reason = 'CancelledByUser', end_time = ?1
             WHERE id = ?2 AND state = 'PENDING'",
            params![now_ts(), job_id],
        )?;
        Ok(changed > 0)
    }

    pub fn node_info(&self) -> Result<NodeInfo> {
        let mut partitions = Vec::new();
        for partition in self.config.active_partitions() {
            partitions.push(self.partition_info(&partition)?);
        }
        Ok(NodeInfo { partitions })
    }

    pub fn running_resource_usage(&self) -> Result<(u32, u64, u32)> {
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(SUM(requested_cpus * requested_tasks), 0), COALESCE(SUM(requested_memory_mb), 0),
                    COALESCE(SUM(requested_gpus), 0)
             FROM jobs
             WHERE state = 'RUNNING' AND parent_job_id IS NULL",
        )?;
        let (cpus, mem, gpus) = stmt.query_row([], |row| {
            let cpus: u32 = row.get(0)?;
            let mem: u64 = row.get(1)?;
            let gpus: u32 = row.get(2)?;
            Ok((cpus, mem, gpus))
        })?;
        Ok((cpus, mem, gpus))
    }

    pub fn available_resources(&self) -> Result<(u32, u64, u32)> {
        let (used_cpus, used_memory, used_gpus) = self.running_resource_usage()?;
        Ok((
            self.config.total_cpus.saturating_sub(used_cpus),
            self.config.total_memory_mb.saturating_sub(used_memory),
            self.config.total_gpus.saturating_sub(used_gpus),
        ))
    }

    pub fn allocate_gpu_ids(&self, requested_gpus: u32) -> Result<Vec<u32>> {
        if requested_gpus == 0 {
            return Ok(Vec::new());
        }

        let mut used = self.used_gpu_ids()?;
        used.sort_unstable();

        let mut assigned = Vec::new();
        for gpu_id in 0..self.config.total_gpus {
            if used.binary_search(&gpu_id).is_err() {
                assigned.push(gpu_id);
                if assigned.len() == requested_gpus as usize {
                    break;
                }
            }
        }

        if assigned.len() == requested_gpus as usize {
            Ok(assigned)
        } else {
            Err(SlotdError::from(
                "not enough free GPU IDs to satisfy the request",
            ))
        }
    }

    fn ensure_compat_schema(&self) -> Result<()> {
        ensure_column(
            &self.conn,
            "jobs",
            "parent_job_id",
            "ALTER TABLE jobs ADD COLUMN parent_job_id INTEGER",
        )?;
        ensure_column(
            &self.conn,
            "jobs",
            "step_id",
            "ALTER TABLE jobs ADD COLUMN step_id INTEGER",
        )?;
        ensure_column(
            &self.conn,
            "jobs",
            "held",
            "ALTER TABLE jobs ADD COLUMN held INTEGER NOT NULL DEFAULT 0",
        )?;
        ensure_column(
            &self.conn,
            "jobs",
            "priority",
            "ALTER TABLE jobs ADD COLUMN priority INTEGER NOT NULL DEFAULT 0",
        )?;
        ensure_column(
            &self.conn,
            "jobs",
            "partition",
            "ALTER TABLE jobs ADD COLUMN partition TEXT NOT NULL DEFAULT 'cpu'",
        )?;
        ensure_column(
            &self.conn,
            "jobs",
            "user_name",
            "ALTER TABLE jobs ADD COLUMN user_name TEXT NOT NULL DEFAULT 'unknown'",
        )?;
        ensure_column(
            &self.conn,
            "jobs",
            "requested_tasks",
            "ALTER TABLE jobs ADD COLUMN requested_tasks INTEGER NOT NULL DEFAULT 1",
        )?;
        ensure_column(
            &self.conn,
            "jobs",
            "requested_gpus",
            "ALTER TABLE jobs ADD COLUMN requested_gpus INTEGER NOT NULL DEFAULT 0",
        )?;
        ensure_column(
            &self.conn,
            "jobs",
            "allocation_only",
            "ALTER TABLE jobs ADD COLUMN allocation_only INTEGER NOT NULL DEFAULT 0",
        )?;
        ensure_column(
            &self.conn,
            "jobs",
            "assigned_gpus",
            "ALTER TABLE jobs ADD COLUMN assigned_gpus TEXT NOT NULL DEFAULT ''",
        )?;
        ensure_column(
            &self.conn,
            "jobs",
            "state_reason",
            "ALTER TABLE jobs ADD COLUMN state_reason TEXT NOT NULL DEFAULT ''",
        )?;
        ensure_column(
            &self.conn,
            "jobs",
            "term_signal",
            "ALTER TABLE jobs ADD COLUMN term_signal INTEGER",
        )?;
        ensure_column(
            &self.conn,
            "jobs",
            "time_limit_secs",
            "ALTER TABLE jobs ADD COLUMN time_limit_secs INTEGER",
        )?;
        ensure_column(
            &self.conn,
            "jobs",
            "dependency",
            "ALTER TABLE jobs ADD COLUMN dependency TEXT",
        )?;
        ensure_column(
            &self.conn,
            "jobs",
            "array_job_id",
            "ALTER TABLE jobs ADD COLUMN array_job_id INTEGER",
        )?;
        ensure_column(
            &self.conn,
            "jobs",
            "array_task_id",
            "ALTER TABLE jobs ADD COLUMN array_task_id INTEGER",
        )?;
        ensure_column(
            &self.conn,
            "jobs",
            "array_task_count",
            "ALTER TABLE jobs ADD COLUMN array_task_count INTEGER",
        )?;
        ensure_column(
            &self.conn,
            "jobs",
            "array_task_limit",
            "ALTER TABLE jobs ADD COLUMN array_task_limit INTEGER",
        )?;
        ensure_column(
            &self.conn,
            "jobs",
            "max_rss_kb",
            "ALTER TABLE jobs ADD COLUMN max_rss_kb INTEGER",
        )?;
        Ok(())
    }

    fn partition_info(&self, partition: &str) -> Result<crate::job::PartitionInfo> {
        let (allocated_cpus, allocated_memory_mb, allocated_gpus) =
            self.running_usage_for_partition(partition)?;
        let running_jobs = self.count_jobs_by_partition_and_state(partition, JobState::Running)?;
        let pending_jobs = self.count_jobs_by_partition_and_state(partition, JobState::Pending)?;
        let is_gpu_partition = self.config.is_gpu_partition(partition);
        let state = partition_state(
            is_gpu_partition,
            allocated_cpus,
            allocated_gpus,
            self.config.total_cpus,
            self.config.total_gpus,
        );
        let gres_used = partition_gres_used(
            is_gpu_partition,
            &self.config.gpu_model,
            self.config.total_gpus,
            &self.used_gpu_ids_for_partition(partition)?,
        );
        Ok(crate::job::PartitionInfo {
            name: partition.to_string(),
            hostname: self.config.hostname.clone(),
            state,
            gres_used,
            total_cpus: self.config.total_cpus,
            total_memory_mb: self.config.total_memory_mb,
            total_gpus: if self.config.is_gpu_partition(partition) {
                self.config.total_gpus
            } else {
                0
            },
            allocated_cpus,
            allocated_memory_mb,
            allocated_gpus,
            running_jobs,
            pending_jobs,
        })
    }

    fn running_usage_for_partition(&self, partition: &str) -> Result<(u32, u64, u32)> {
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(SUM(requested_cpus * requested_tasks), 0), COALESCE(SUM(requested_memory_mb), 0),
                    COALESCE(SUM(requested_gpus), 0)
             FROM jobs
             WHERE state = 'RUNNING' AND parent_job_id IS NULL AND partition = ?1",
        )?;
        let usage = stmt.query_row([partition], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        Ok(usage)
    }

    fn used_gpu_ids(&self) -> Result<Vec<u32>> {
        let mut stmt = self.conn.prepare(
            "SELECT assigned_gpus FROM jobs WHERE state = 'RUNNING' AND parent_job_id IS NULL AND assigned_gpus <> ''",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut ids = Vec::new();
        for row in rows {
            ids.extend(parse_gpu_ids(&row?)?);
        }
        Ok(ids)
    }

    fn used_gpu_ids_for_partition(&self, partition: &str) -> Result<Vec<u32>> {
        let mut stmt = self.conn.prepare(
            "SELECT assigned_gpus FROM jobs
             WHERE state = 'RUNNING' AND parent_job_id IS NULL AND partition = ?1 AND assigned_gpus <> ''",
        )?;
        let rows = stmt.query_map([partition], |row| row.get::<_, String>(0))?;
        let mut ids = Vec::new();
        for row in rows {
            ids.extend(parse_gpu_ids(&row?)?);
        }
        ids.sort_unstable();
        Ok(ids)
    }

    fn count_jobs_by_partition_and_state(&self, partition: &str, state: JobState) -> Result<usize> {
        let count = self.conn.query_row(
            "SELECT COUNT(*) FROM jobs WHERE partition = ?1 AND state = ?2 AND parent_job_id IS NULL",
            params![partition, state.as_str()],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(count as usize)
    }
}

fn map_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<JobRecord> {
    let state_text: String = row.get(7)?;
    let state = state_text.parse().map_err(|message: String| {
        rusqlite::Error::FromSqlConversionFailure(
            7,
            rusqlite::types::Type::Text,
            Box::new(SlotdError::from(message)),
        )
    })?;
    Ok(JobRecord {
        id: row.get(0)?,
        parent_job_id: row.get(1)?,
        step_id: row.get::<_, Option<i64>>(2)?.map(|value| value as u32),
        held: row.get::<_, bool>(3)?,
        priority: row.get(4)?,
        name: row.get(5)?,
        user_name: row.get(6)?,
        state,
        partition: row.get(8)?,
        command: row.get(9)?,
        cwd: row.get(10)?,
        requested_cpus: row.get(11)?,
        requested_memory_mb: row.get(12)?,
        requested_tasks: row.get(13)?,
        requested_gpus: row.get(14)?,
        allocation_only: row.get::<_, bool>(15)?,
        dependency: row.get(16)?,
        array_job_id: row.get(17)?,
        array_task_id: row.get(18)?,
        array_task_count: row.get::<_, Option<i64>>(19)?.map(|value| value as u32),
        array_task_limit: row.get::<_, Option<i64>>(20)?.map(|value| value as u32),
        max_rss_kb: row.get::<_, Option<i64>>(21)?.map(|value| value as u64),
        submit_time: row.get(22)?,
        start_time: row.get(23)?,
        end_time: row.get(24)?,
        pid: row.get(25)?,
        pgid: row.get(26)?,
        exit_code: row.get(27)?,
        state_reason: row
            .get::<_, String>(28)
            .ok()
            .filter(|value| !value.is_empty()),
        term_signal: row.get(29)?,
        time_limit_secs: row.get::<_, Option<i64>>(30)?.map(|value| value as u64),
        assigned_gpu_ids: parse_gpu_ids(&row.get::<_, String>(31)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                31,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        script_path: row.get(32)?,
        stdout_path: row.get(33)?,
        stderr_path: row.get(34)?,
    })
}

fn now_ts() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn default_name(script_name: &str) -> String {
    Path::new(script_name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("batch-job")
        .to_string()
}

fn default_output_pattern(array_task_id: Option<i32>) -> &'static str {
    if array_task_id.is_some() {
        "slurm-%A_%a.out"
    } else {
        default_batch_output_pattern()
    }
}

fn next_step_id_query(conn: &Connection, parent_job_id: i64) -> Result<u32> {
    let current = conn.query_row(
        "SELECT COALESCE(MAX(step_id), -1) FROM jobs WHERE parent_job_id = ?1",
        [parent_job_id],
        |row| row.get::<_, i64>(0),
    )?;
    Ok((current + 1).max(0) as u32)
}

fn order_pending_jobs(mut jobs: Vec<JobRecord>) -> Vec<JobRecord> {
    let now = now_ts();
    jobs.sort_by(|a, b| {
        let a_score = effective_priority(a, now);
        let b_score = effective_priority(b, now);
        b_score
            .cmp(&a_score)
            .then_with(|| a.submit_time.cmp(&b.submit_time))
            .then_with(|| a.id.cmp(&b.id))
    });

    let mut grouped = std::collections::BTreeMap::<i64, Vec<JobRecord>>::new();
    for job in jobs {
        let group_key = job.array_job_id.unwrap_or(job.id);
        grouped.entry(group_key).or_default().push(job);
    }

    let group_order = grouped
        .iter()
        .map(|(group_key, group_jobs)| {
            let top = &group_jobs[0];
            (*group_key, effective_priority(top, now), top.submit_time, top.id)
        })
        .collect::<Vec<_>>();

    let mut group_order = group_order;
    group_order.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.2.cmp(&b.2)).then_with(|| a.3.cmp(&b.3)));

    let mut ordered = Vec::new();
    loop {
        let mut progressed = false;
        for (group_key, _, _, _) in &group_order {
            if let Some(group_jobs) = grouped.get_mut(group_key) {
                if !group_jobs.is_empty() {
                    ordered.push(group_jobs.remove(0));
                    progressed = true;
                }
            }
        }
        if !progressed {
            break;
        }
    }

    ordered
}

fn effective_priority(job: &JobRecord, now: i64) -> i64 {
    let age_bonus = now.saturating_sub(job.submit_time) / 60;
    job.priority as i64 + age_bonus
}

#[cfg(test)]
mod tests {
    use super::{effective_priority, order_pending_jobs};
    use crate::job::{JobRecord, JobState};

    fn pending_job(id: i64, array_job_id: Option<i64>, priority: i32, submit_time: i64) -> JobRecord {
        JobRecord {
            id,
            parent_job_id: None,
            step_id: None,
            held: false,
            priority,
            name: format!("job-{id}"),
            user_name: "test".to_string(),
            state: JobState::Pending,
            partition: "cpu".to_string(),
            command: "true".to_string(),
            cwd: "/tmp".to_string(),
            requested_cpus: 1,
            requested_tasks: 1,
            requested_memory_mb: 1,
            requested_gpus: 0,
            allocation_only: false,
            dependency: None,
            array_job_id,
            array_task_id: None,
            array_task_count: None,
            array_task_limit: None,
            max_rss_kb: None,
            submit_time,
            start_time: None,
            end_time: None,
            pid: None,
            pgid: None,
            exit_code: None,
            state_reason: None,
            term_signal: None,
            time_limit_secs: None,
            assigned_gpu_ids: Vec::new(),
            script_path: String::new(),
            stdout_path: String::new(),
            stderr_path: String::new(),
        }
    }

    #[test]
    fn scheduler_interleaves_array_groups() {
        let ordered = order_pending_jobs(vec![
            pending_job(1, Some(1), 0, 0),
            pending_job(2, Some(1), 0, 1),
            pending_job(3, Some(3), 0, 2),
            pending_job(4, Some(3), 0, 3),
        ]);
        let ids = ordered.into_iter().map(|job| job.id).collect::<Vec<_>>();
        assert_eq!(ids, vec![1, 3, 2, 4]);
    }

    #[test]
    fn scheduler_prefers_higher_effective_priority() {
        let newer = pending_job(10, None, 100, 100);
        let older = pending_job(11, None, 0, 0);
        assert!(effective_priority(&newer, 100) > effective_priority(&older, 100));
        let ordered = order_pending_jobs(vec![older, newer]);
        assert_eq!(ordered[0].id, 10);
    }
}

fn script_command(script_name: &str) -> String {
    format!("bash {script_name}")
}

fn path_string(path: &PathBuf) -> String {
    path.to_string_lossy().to_string()
}

fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn ensure_column(conn: &Connection, table: &str, column: &str, alter_sql: &str) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let mut exists = false;
    for entry in columns {
        if entry? == column {
            exists = true;
            break;
        }
    }

    if !exists {
        conn.execute_batch(alter_sql)?;
    }
    Ok(())
}

fn join_gpu_ids(ids: &[u32]) -> String {
    ids.iter().map(u32::to_string).collect::<Vec<_>>().join(",")
}

fn parse_gpu_ids(value: &str) -> Result<Vec<u32>> {
    if value.trim().is_empty() {
        return Ok(Vec::new());
    }

    value
        .split(',')
        .map(|part| {
            part.trim()
                .parse::<u32>()
                .map_err(|_| SlotdError::from(format!("invalid GPU id list: {value}")))
        })
        .collect()
}

fn filter_jobs(
    jobs: Vec<JobRecord>,
    states: Option<&[JobState]>,
    ids: Option<&[i64]>,
    user_name: Option<&str>,
    partitions: Option<&[String]>,
    start_time: Option<i64>,
    end_time: Option<i64>,
) -> Vec<JobRecord> {
    jobs.into_iter()
        .filter(|job| {
            let state_ok = states
                .map(|states| states.contains(&job.state))
                .unwrap_or(true);
            let id_ok = ids.map(|ids| ids.contains(&job.id)).unwrap_or(true);
            let user_ok = user_name.map(|name| job.user_name == name).unwrap_or(true);
            let partition_ok = partitions
                .map(|partitions| {
                    partitions
                        .iter()
                        .any(|partition| partition == &job.partition)
                })
                .unwrap_or(true);
            let start_ok = start_time
                .map(|start| {
                    job.submit_time >= start || job.start_time.unwrap_or(job.submit_time) >= start
                })
                .unwrap_or(true);
            let end_ok = end_time
                .map(|end| {
                    let effective_end = job.end_time.or(job.start_time).unwrap_or(job.submit_time);
                    effective_end <= end
                })
                .unwrap_or(true);
            state_ok && id_ok && user_ok && partition_ok && start_ok && end_ok
        })
        .collect()
}

fn partition_state(
    is_gpu_partition: bool,
    allocated_cpus: u32,
    allocated_gpus: u32,
    total_cpus: u32,
    total_gpus: u32,
) -> String {
    if is_gpu_partition {
        if allocated_gpus == 0 {
            "idle".to_string()
        } else if allocated_gpus >= total_gpus {
            "alloc".to_string()
        } else {
            "mix".to_string()
        }
    } else if allocated_cpus == 0 {
        "idle".to_string()
    } else if allocated_cpus >= total_cpus {
        "alloc".to_string()
    } else {
        "mix".to_string()
    }
}

fn partition_gres_used(
    is_gpu_partition: bool,
    gpu_model: &str,
    total_gpus: u32,
    ids: &[u32],
) -> String {
    if !is_gpu_partition {
        return "N/A".to_string();
    }

    let idx = if ids.is_empty() {
        "IDX:N/A".to_string()
    } else {
        format!(
            "IDX:{}",
            ids.iter().map(u32::to_string).collect::<Vec<_>>().join(",")
        )
    };

    format!("gpu:{gpu_model}:{total_gpus}({idx})")
}
