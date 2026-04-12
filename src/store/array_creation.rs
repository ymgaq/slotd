use crate::app::error::{Result, SlotdError};
use crate::model::job::SubmitRequest;
use crate::store::support::default_name;
use crate::submit::sbatch::parse_array_spec;

use super::Store;

impl Store {
    pub(super) fn create_array_jobs(&self, request: SubmitRequest) -> Result<i64> {
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
}
