use crate::app::error::Result;
use crate::model::job::JobRecord;

use super::{JOB_SELECT_COLUMNS, Store, map_job};
use rusqlite::OptionalExtension;

impl Store {
    pub fn get_job(&self, job_id: i64) -> Result<Option<JobRecord>> {
        self.conn
            .query_row(
                &format!(
                    "SELECT {JOB_SELECT_COLUMNS}
                 FROM jobs
                 WHERE id = ?1"
                ),
                [job_id],
                map_job,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn list_running_jobs(&self) -> Result<Vec<JobRecord>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {JOB_SELECT_COLUMNS}
             FROM jobs
             WHERE state = 'RUNNING'
             ORDER BY id ASC"
        ))?;
        let rows = stmt.query_map([], map_job)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }
}
