use crate::app::error::{Result, SlotdError};
use crate::model::job::JobRecord;
use crate::util::time::now_ts;
use rusqlite::params;

use super::{JOB_SELECT_COLUMNS, Store, next_step_id_query};

impl Store {
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
                submit_time, start_time, state_reason, time_limit_secs, begin_time, exclusive, script_path, stdout_path, stderr_path,
                export_env, open_mode, warning_signal, warning_signal_seconds, [constraint], cpu_bind, requeue, requeue_count
            ) VALUES (?1, ?2, 0, 0, ?3, ?4, 'RUNNING', ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0, NULL, NULL, NULL, NULL, NULL, ?12, ?13, '', NULL, NULL, 0, '', '', '', '', 'truncate', NULL, NULL, NULL, NULL, 0, 0)",
            params![
                parent_job_id,
                next_step_id as i64,
                name,
                user_name,
                parent.partition,
                command,
                cwd,
                parent.requested_cpus,
                parent.requested_memory_mb,
                parent.requested_tasks,
                parent.requested_gpus,
                submit_time,
                submit_time,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_steps(&self, parent_job_id: i64) -> Result<Vec<JobRecord>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {JOB_SELECT_COLUMNS}
             FROM jobs
             WHERE parent_job_id = ?1
             ORDER BY step_id ASC, id ASC"
        ))?;
        let rows = stmt.query_map([parent_job_id], super::row::map_job)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    fn next_step_id(&self, parent_job_id: i64) -> Result<u32> {
        next_step_id_query(&self.conn, parent_job_id)
    }
}
