use crate::app::error::Result;
use crate::store::support::parse_gpu_ids;

use super::Store;

impl Store {
    pub(super) fn running_usage_totals(&self, partition: Option<&str>) -> Result<(u32, u64, u32)> {
        let sql = if partition.is_some() {
            "SELECT COALESCE(SUM(requested_cpus * requested_tasks), 0), COALESCE(SUM(requested_memory_mb), 0),
                    COALESCE(SUM(requested_gpus), 0)
             FROM jobs
             WHERE state = 'RUNNING' AND parent_job_id IS NULL AND partition = ?1"
        } else {
            "SELECT COALESCE(SUM(requested_cpus * requested_tasks), 0), COALESCE(SUM(requested_memory_mb), 0),
                    COALESCE(SUM(requested_gpus), 0)
             FROM jobs
             WHERE state = 'RUNNING' AND parent_job_id IS NULL"
        };
        let mut stmt = self.conn.prepare(sql)?;
        if let Some(partition) = partition {
            stmt.query_row([partition], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map_err(Into::into)
        } else {
            stmt.query_row([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
                .map_err(Into::into)
        }
    }

    pub(super) fn gpu_ids_in_use(&self, partition: Option<&str>) -> Result<Vec<u32>> {
        let mut ids = Vec::new();
        if let Some(partition) = partition {
            let mut stmt = self.conn.prepare(
                "SELECT assigned_gpus FROM jobs
                 WHERE state = 'RUNNING' AND parent_job_id IS NULL AND partition = ?1 AND assigned_gpus <> ''",
            )?;
            let rows = stmt.query_map([partition], |row| row.get::<_, String>(0))?;
            for row in rows {
                let value = row?;
                ids.extend(parse_gpu_ids(&value)?);
            }
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT assigned_gpus FROM jobs
                 WHERE state = 'RUNNING' AND parent_job_id IS NULL AND assigned_gpus <> ''",
            )?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            for row in rows {
                let value = row?;
                ids.extend(parse_gpu_ids(&value)?);
            }
        }
        Ok(ids)
    }
}
