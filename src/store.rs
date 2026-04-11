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
        Ok(Self { conn, config })
    }

    pub fn create_job(&self, request: SubmitRequest) -> Result<i64> {
        let submit_time = now_ts();
        let resolved_name = request
            .name
            .clone()
            .unwrap_or_else(|| default_name(&request.script_name));
        self.conn.execute(
            "INSERT INTO jobs (
                name, state, command, cwd, requested_cpus, requested_memory_mb, submit_time
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                resolved_name,
                JobState::Pending.as_str(),
                script_command(&request.script_name),
                request.cwd,
                request.requested_cpus,
                request.requested_memory_mb,
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
            "SELECT id, name, state, command, cwd, requested_cpus, requested_memory_mb,
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
            "SELECT id, name, state, command, cwd, requested_cpus, requested_memory_mb,
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

    pub fn next_pending_job(
        &self,
        available_cpus: u32,
        available_memory_mb: u64,
    ) -> Result<Option<JobRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, state, command, cwd, requested_cpus, requested_memory_mb,
                    submit_time, start_time, end_time, pid, pgid, exit_code,
                    script_path, stdout_path, stderr_path
             FROM jobs
             WHERE state = 'PENDING'
               AND requested_cpus <= ?1
               AND requested_memory_mb <= ?2
             ORDER BY id ASC
             LIMIT 1",
        )?;
        stmt.query_row(params![available_cpus, available_memory_mb], map_job)
            .optional()
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
        let running_jobs = self.count_jobs_by_state(JobState::Running)?;
        let pending_jobs = self.count_jobs_by_state(JobState::Pending)?;
        let (allocated_cpus, allocated_memory_mb) = self.running_resource_usage()?;
        Ok(NodeInfo {
            total_cpus: self.config.total_cpus,
            total_memory_mb: self.config.total_memory_mb,
            allocated_cpus,
            allocated_memory_mb,
            running_jobs,
            pending_jobs,
        })
    }

    pub fn running_resource_usage(&self) -> Result<(u32, u64)> {
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(SUM(requested_cpus), 0), COALESCE(SUM(requested_memory_mb), 0)
             FROM jobs
             WHERE state = 'RUNNING'",
        )?;
        let (cpus, mem) = stmt.query_row([], |row| {
            let cpus: u32 = row.get(0)?;
            let mem: u64 = row.get(1)?;
            Ok((cpus, mem))
        })?;
        Ok((cpus, mem))
    }

    fn count_jobs_by_state(&self, state: JobState) -> Result<usize> {
        let count = self.conn.query_row(
            "SELECT COUNT(*) FROM jobs WHERE state = ?1",
            [state.as_str()],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(count as usize)
    }

    pub fn available_resources(&self) -> Result<(u32, u64)> {
        let (used_cpus, used_memory) = self.running_resource_usage()?;
        Ok((
            self.config.total_cpus.saturating_sub(used_cpus),
            self.config.total_memory_mb.saturating_sub(used_memory),
        ))
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
        command: row.get(3)?,
        cwd: row.get(4)?,
        requested_cpus: row.get(5)?,
        requested_memory_mb: row.get(6)?,
        submit_time: row.get(7)?,
        start_time: row.get(8)?,
        end_time: row.get(9)?,
        pid: row.get(10)?,
        pgid: row.get(11)?,
        exit_code: row.get(12)?,
        script_path: row.get(13)?,
        stdout_path: row.get(14)?,
        stderr_path: row.get(15)?,
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
