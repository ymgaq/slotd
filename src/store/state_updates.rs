use rusqlite::params;

use crate::app::error::{Result, SlotdError};
use crate::model::job::{JobRecord, JobState};
use crate::store::support::join_gpu_ids;
use crate::util::time::now_ts;

use super::{Store, should_auto_requeue};

impl Store {
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
    ) -> Result<JobRecord> {
        let job = self
            .get_job(job_id)?
            .ok_or_else(|| SlotdError::from(format!("job {job_id} not found")))?;
        if should_auto_requeue(&job, state) {
            self.conn.execute(
                "UPDATE jobs
                 SET state = 'PENDING', exit_code = NULL, term_signal = NULL, state_reason = 'Requeued',
                     start_time = NULL, end_time = NULL, pid = NULL, pgid = NULL, assigned_gpus = '',
                     max_rss_kb = NULL, requeue_count = requeue_count + 1
                 WHERE id = ?1",
                [job_id],
            )?;
        } else {
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
        }
        self.get_job(job_id)?
            .ok_or_else(|| SlotdError::from(format!("job {job_id} not found after update")))
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

    pub fn cancel_pending_job(&self, job_id: i64) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE jobs
             SET state = 'CANCELLED', state_reason = 'CancelledByUser', end_time = ?1
             WHERE id = ?2 AND state = 'PENDING'",
            params![now_ts(), job_id],
        )?;
        Ok(changed > 0)
    }
}
