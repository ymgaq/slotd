use rusqlite::{OptionalExtension, params};

use crate::app::error::Result;
use crate::model::job::{JobRecord, JobState};

use super::{JOB_SELECT_COLUMNS, Store, map_job};
use crate::store::support::{filter_jobs, order_pending_jobs};

impl Store {
    pub fn list_jobs(
        &self,
        states: Option<&[JobState]>,
        ids: Option<&[i64]>,
        user_name: Option<&str>,
        partitions: Option<&[String]>,
    ) -> Result<Vec<JobRecord>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {JOB_SELECT_COLUMNS}
             FROM jobs
             WHERE parent_job_id IS NULL"
        ))?;
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
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {JOB_SELECT_COLUMNS} FROM jobs"))?;
        let rows = stmt.query_map([], map_job)?;
        let mut jobs = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        jobs.sort_by(|a, b| b.id.cmp(&a.id));
        Ok(filter_jobs(
            jobs, states, ids, user_name, partitions, start_time, end_time,
        ))
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

    pub fn next_pending_jobs(&self) -> Result<Vec<JobRecord>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {JOB_SELECT_COLUMNS}
             FROM jobs
             WHERE state = 'PENDING' AND parent_job_id IS NULL
             ORDER BY id ASC
            "
        ))?;
        let rows = stmt.query_map([], map_job)?;
        let jobs = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(order_pending_jobs(jobs))
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

    pub fn any_running_top_level_job(&self) -> Result<bool> {
        let count = self.conn.query_row(
            "SELECT COUNT(*) FROM jobs WHERE state = 'RUNNING' AND parent_job_id IS NULL",
            [],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(count > 0)
    }

    pub fn any_running_exclusive_job(&self) -> Result<bool> {
        let count = self.conn.query_row(
            "SELECT COUNT(*) FROM jobs WHERE state = 'RUNNING' AND parent_job_id IS NULL AND exclusive = 1",
            [],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(count > 0)
    }
}
