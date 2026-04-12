use rusqlite::params;

use crate::app::error::{Result, SlotdError};
use crate::model::job::JobState;

use super::Store;

impl Store {
    pub fn hold_job(&self, job_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE jobs SET held = 1, state_reason = 'JobHeldUser' WHERE id = ?1 AND state = 'PENDING'",
            [job_id],
        )?;
        Ok(())
    }

    pub fn release_job(&self, job_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE jobs SET held = 0, state_reason = 'Resources' WHERE id = ?1 AND state = 'PENDING'",
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
                return Err(SlotdError::from(
                    "job name can only be updated while pending",
                ));
            }
            self.conn.execute(
                "UPDATE jobs SET name = ?1 WHERE id = ?2",
                params![value, job_id],
            )?;
        }
        if let Some(value) = partition {
            if job.state != JobState::Pending {
                return Err(SlotdError::from(
                    "partition can only be updated while pending",
                ));
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
                return Err(SlotdError::from(
                    "priority can only be updated while pending",
                ));
            }
            self.conn.execute(
                "UPDATE jobs SET priority = ?1 WHERE id = ?2",
                params![value, job_id],
            )?;
        }
        Ok(())
    }
}
