use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use crate::config::AppConfig;
use crate::error::{Result, SlotdError};
use crate::job::{JobRecord, JobState, NodeInfo, SubmitRequest};
use crate::sbatch::resolve_log_path;

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

    pub fn create_job(&self, request: SubmitRequest) -> Result<i64> {
        let submit_time = now_ts();
        let resolved_name = request
            .name
            .clone()
            .unwrap_or_else(|| default_name(&request.script_name));
        self.conn.execute(
            "INSERT INTO jobs (
                name, state, partition, command, cwd, requested_cpus, requested_memory_mb,
                requested_gpus, submit_time
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                resolved_name,
                JobState::Pending.as_str(),
                request.partition,
                request
                    .command_override
                    .clone()
                    .unwrap_or_else(|| script_command(&request.script_name)),
                request.cwd,
                request.requested_cpus,
                request.requested_memory_mb,
                request.requested_gpus,
                submit_time,
            ],
        )?;

        let job_id = self.conn.last_insert_rowid();
        let job_dir = self.config.jobs_dir.join(job_id.to_string());
        fs::create_dir_all(&job_dir)?;

        let script_path = job_dir.join("script.sh");
        fs::write(&script_path, request.script_body)?;

        let stdout_path = request
            .stdout_path
            .as_deref()
            .map(|path| PathBuf::from(resolve_log_path(&request.cwd, path)))
            .unwrap_or_else(|| job_dir.join("stdout.log"));
        let stderr_path = request
            .stderr_path
            .as_deref()
            .map(|path| PathBuf::from(resolve_log_path(&request.cwd, path)))
            .unwrap_or_else(|| job_dir.join("stderr.log"));
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

    pub fn list_jobs(&self) -> Result<Vec<JobRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, state, partition, command, cwd, requested_cpus, requested_memory_mb,
                    requested_gpus,
                    submit_time, start_time, end_time, pid, pgid, exit_code,
                    script_path, stdout_path, stderr_path
             FROM jobs
             ORDER BY id DESC",
        )?;
        let rows = stmt.query_map([], map_job)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn list_running_jobs(&self) -> Result<Vec<JobRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, state, partition, command, cwd, requested_cpus, requested_memory_mb,
                    requested_gpus,
                    submit_time, start_time, end_time, pid, pgid, exit_code,
                    script_path, stdout_path, stderr_path
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
                "SELECT id, name, state, partition, command, cwd, requested_cpus, requested_memory_mb,
                        requested_gpus,
                        submit_time, start_time, end_time, pid, pgid, exit_code,
                        script_path, stdout_path, stderr_path
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
            "SELECT id, name, state, partition, command, cwd, requested_cpus, requested_memory_mb,
                    requested_gpus, submit_time, start_time, end_time, pid, pgid, exit_code,
                    script_path, stdout_path, stderr_path
             FROM jobs
             WHERE state = 'PENDING'
             ORDER BY id ASC
            ",
        )?;
        let rows = stmt.query_map([], map_job)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn mark_running(&self, job_id: i64, pid: i32, pgid: i32) -> Result<()> {
        self.conn.execute(
            "UPDATE jobs
             SET state = 'RUNNING', pid = ?1, pgid = ?2, start_time = ?3
             WHERE id = ?4",
            params![pid, pgid, now_ts(), job_id],
        )?;
        Ok(())
    }

    pub fn mark_finished(
        &self,
        job_id: i64,
        state: JobState,
        exit_code: Option<i32>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE jobs
             SET state = ?1, exit_code = ?2, end_time = ?3
             WHERE id = ?4",
            params![state.as_str(), exit_code, now_ts(), job_id],
        )?;
        Ok(())
    }

    pub fn cancel_pending_job(&self, job_id: i64) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE jobs
             SET state = 'CANCELLED', end_time = ?1
             WHERE id = ?2 AND state = 'PENDING'",
            params![now_ts(), job_id],
        )?;
        Ok(changed > 0)
    }

    pub fn node_info(&self) -> Result<NodeInfo> {
        let cpu = self.partition_info("cpu")?;
        let gpu = self.partition_info("gpu")?;
        Ok(NodeInfo {
            partitions: vec![cpu, gpu],
        })
    }

    pub fn running_resource_usage(&self) -> Result<(u32, u64, u32)> {
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(SUM(requested_cpus), 0), COALESCE(SUM(requested_memory_mb), 0),
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
            "requested_gpus",
            "ALTER TABLE jobs ADD COLUMN requested_gpus INTEGER NOT NULL DEFAULT 0",
        )?;
        Ok(())
    }

    fn partition_info(&self, partition: &str) -> Result<crate::job::PartitionInfo> {
        let (allocated_cpus, allocated_memory_mb, allocated_gpus) =
            self.running_usage_for_partition(partition)?;
        let running_jobs = self.count_jobs_by_partition_and_state(partition, JobState::Running)?;
        let pending_jobs = self.count_jobs_by_partition_and_state(partition, JobState::Pending)?;
        Ok(crate::job::PartitionInfo {
            name: partition.to_string(),
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
            "SELECT COALESCE(SUM(requested_cpus), 0), COALESCE(SUM(requested_memory_mb), 0),
                    COALESCE(SUM(requested_gpus), 0)
             FROM jobs
             WHERE state = 'RUNNING' AND partition = ?1",
        )?;
        let usage = stmt.query_row([partition], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        Ok(usage)
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
    let state_text: String = row.get(2)?;
    let state = state_text.parse().map_err(|message: String| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            Box::new(SlotdError::from(message)),
        )
    })?;
    Ok(JobRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        state,
        partition: row.get(3)?,
        command: row.get(4)?,
        cwd: row.get(5)?,
        requested_cpus: row.get(6)?,
        requested_memory_mb: row.get(7)?,
        requested_gpus: row.get(8)?,
        submit_time: row.get(9)?,
        start_time: row.get(10)?,
        end_time: row.get(11)?,
        pid: row.get(12)?,
        pgid: row.get(13)?,
        exit_code: row.get(14)?,
        script_path: row.get(15)?,
        stdout_path: row.get(16)?,
        stderr_path: row.get(17)?,
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
