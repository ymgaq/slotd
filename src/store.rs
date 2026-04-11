use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use crate::config::AppConfig;
use crate::error::{Result, SlotdError};
use crate::job::{JobRecord, JobState, NodeInfo, SubmitRequest};
use crate::sbatch::{default_batch_output_pattern, expand_output_pattern, resolve_log_path};

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
        let submit_time = now_ts();
        let user_name = request.user_name.clone();
        let resolved_name = request
            .name
            .clone()
            .unwrap_or_else(|| default_name(&request.script_name));
        self.conn.execute(
            "INSERT INTO jobs (
                name, user_name, state, partition, command, cwd, requested_cpus, requested_memory_mb,
                requested_tasks, requested_gpus, allocation_only, submit_time, state_reason, time_limit_secs
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                resolved_name,
                request.user_name,
                JobState::Pending.as_str(),
                request.partition,
                request
                    .command_override
                    .clone()
                    .unwrap_or_else(|| script_command(&request.script_name)),
                request.cwd,
                request.requested_cpus,
                request.requested_memory_mb,
                request.requested_tasks,
                request.requested_gpus,
                request.allocation_only,
                submit_time,
                "Resources",
                request.time_limit_secs.map(|value| value as i64),
            ],
        )?;

        let job_id = self.conn.last_insert_rowid();
        let job_dir = self.config.jobs_dir.join(job_id.to_string());
        fs::create_dir_all(&job_dir)?;

        let script_path = job_dir.join("script.sh");
        fs::write(&script_path, request.script_body)?;

        let default_stdout = expand_output_pattern(
            default_batch_output_pattern(),
            job_id,
            &resolved_name,
            &user_name,
            &self.config.hostname,
        );
        let stdout_path = request
            .stdout_path
            .as_deref()
            .map(|path| {
                expand_output_pattern(path, job_id, &resolved_name, &user_name, &self.config.hostname)
            })
            .unwrap_or(default_stdout);
        let stdout_path = PathBuf::from(resolve_log_path(&request.cwd, &stdout_path));
        let stderr_path = request
            .stderr_path
            .as_deref()
            .map(|path| {
                expand_output_pattern(path, job_id, &resolved_name, &user_name, &self.config.hostname)
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

    pub fn list_jobs(
        &self,
        states: Option<&[JobState]>,
        ids: Option<&[i64]>,
        user_name: Option<&str>,
        partitions: Option<&[String]>,
    ) -> Result<Vec<JobRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, user_name, state, partition, command, cwd, requested_cpus, requested_memory_mb,
                    requested_tasks, requested_gpus, allocation_only,
                    submit_time, start_time, end_time, pid, pgid, exit_code, state_reason, term_signal, time_limit_secs,
                    assigned_gpus, script_path, stdout_path, stderr_path
             FROM jobs"
        )?;
        let rows = stmt.query_map([], map_job)?;
        let mut jobs = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        jobs.sort_by(|a, b| b.id.cmp(&a.id));
        Ok(filter_jobs(
            jobs,
            states,
            ids,
            user_name,
            partitions,
            None,
            None,
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
            "SELECT id, name, user_name, state, partition, command, cwd, requested_cpus, requested_memory_mb,
                    requested_tasks, requested_gpus, allocation_only,
                    submit_time, start_time, end_time, pid, pgid, exit_code, state_reason, term_signal, time_limit_secs,
                    assigned_gpus, script_path, stdout_path, stderr_path
             FROM jobs"
        )?;
        let rows = stmt.query_map([], map_job)?;
        let mut jobs = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        jobs.sort_by(|a, b| b.id.cmp(&a.id));
        Ok(filter_jobs(
            jobs,
            states,
            ids,
            user_name,
            partitions,
            start_time,
            end_time,
        ))
    }

    pub fn list_running_jobs(&self) -> Result<Vec<JobRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, user_name, state, partition, command, cwd, requested_cpus, requested_memory_mb,
                    requested_tasks, requested_gpus, allocation_only,
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
                "SELECT id, name, user_name, state, partition, command, cwd, requested_cpus, requested_memory_mb,
                        requested_tasks, requested_gpus, allocation_only,
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
            "SELECT id, name, user_name, state, partition, command, cwd, requested_cpus, requested_memory_mb,
                    requested_tasks, requested_gpus, allocation_only, submit_time, start_time, end_time, pid, pgid, exit_code, state_reason, term_signal, time_limit_secs,
                    assigned_gpus, script_path, stdout_path, stderr_path
             FROM jobs
             WHERE state = 'PENDING'
             ORDER BY id ASC
            ",
        )?;
        let rows = stmt.query_map([], map_job)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
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

    pub fn mark_state(&self, job_id: i64, state: JobState, state_reason: Option<&str>) -> Result<()> {
        self.conn.execute(
            "UPDATE jobs
             SET state = ?1, state_reason = ?2
             WHERE id = ?3",
            params![state.as_str(), state_reason.unwrap_or(""), job_id],
        )?;
        Ok(())
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
             WHERE state = 'RUNNING'",
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
        Ok(())
    }

    fn partition_info(&self, partition: &str) -> Result<crate::job::PartitionInfo> {
        let (allocated_cpus, allocated_memory_mb, allocated_gpus) =
            self.running_usage_for_partition(partition)?;
        let running_jobs = self.count_jobs_by_partition_and_state(partition, JobState::Running)?;
        let pending_jobs = self.count_jobs_by_partition_and_state(partition, JobState::Pending)?;
        let state = partition_state(
            partition,
            allocated_cpus,
            allocated_gpus,
            self.config.total_cpus,
            self.config.total_gpus,
        );
        let gres_used = partition_gres_used(
            partition,
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
            total_gpus: if partition == "gpu" {
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
             WHERE state = 'RUNNING' AND partition = ?1",
        )?;
        let usage = stmt.query_row([partition], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        Ok(usage)
    }

    fn used_gpu_ids(&self) -> Result<Vec<u32>> {
        let mut stmt = self.conn.prepare(
            "SELECT assigned_gpus FROM jobs WHERE state = 'RUNNING' AND assigned_gpus <> ''",
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
             WHERE state = 'RUNNING' AND partition = ?1 AND assigned_gpus <> ''",
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
            "SELECT COUNT(*) FROM jobs WHERE partition = ?1 AND state = ?2",
            params![partition, state.as_str()],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(count as usize)
    }
}

fn map_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<JobRecord> {
    let state_text: String = row.get(3)?;
    let state = state_text.parse().map_err(|message: String| {
        rusqlite::Error::FromSqlConversionFailure(
            3,
            rusqlite::types::Type::Text,
            Box::new(SlotdError::from(message)),
        )
    })?;
    Ok(JobRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        user_name: row.get(2)?,
        state,
        partition: row.get(4)?,
        command: row.get(5)?,
        cwd: row.get(6)?,
        requested_cpus: row.get(7)?,
        requested_memory_mb: row.get(8)?,
        requested_tasks: row.get(9)?,
        requested_gpus: row.get(10)?,
        allocation_only: row.get::<_, bool>(11)?,
        submit_time: row.get(12)?,
        start_time: row.get(13)?,
        end_time: row.get(14)?,
        pid: row.get(15)?,
        pgid: row.get(16)?,
        exit_code: row.get(17)?,
        state_reason: row
            .get::<_, String>(18)
            .ok()
            .filter(|value| !value.is_empty()),
        term_signal: row.get(19)?,
        time_limit_secs: row.get::<_, Option<i64>>(20)?.map(|value| value as u64),
        assigned_gpu_ids: parse_gpu_ids(&row.get::<_, String>(21)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                21,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        script_path: row.get(22)?,
        stdout_path: row.get(23)?,
        stderr_path: row.get(24)?,
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
                .map(|partitions| partitions.iter().any(|partition| partition == &job.partition))
                .unwrap_or(true);
            let start_ok = start_time
                .map(|start| job.submit_time >= start || job.start_time.unwrap_or(job.submit_time) >= start)
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
    partition: &str,
    allocated_cpus: u32,
    allocated_gpus: u32,
    total_cpus: u32,
    total_gpus: u32,
) -> String {
    if partition == "gpu" {
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
    partition: &str,
    gpu_model: &str,
    total_gpus: u32,
    ids: &[u32],
) -> String {
    if partition != "gpu" {
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
