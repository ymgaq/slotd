use rusqlite::params;

use crate::app::error::Result;
use crate::model::job::JobState;
use crate::store::support::parse_gpu_ids;

use super::Store;

pub(super) struct PartitionStats {
    pub(super) allocated_cpus: u32,
    pub(super) allocated_memory_mb: u64,
    pub(super) allocated_gpus: u32,
    pub(super) running_jobs: usize,
    pub(super) pending_jobs: usize,
    pub(super) used_gpu_ids: Vec<u32>,
}

impl Store {
    pub(super) fn load_partition_stats(&self, partition: &str) -> Result<PartitionStats> {
        let (allocated_cpus, allocated_memory_mb, allocated_gpus) =
            self.partition_usage_totals(partition)?;
        Ok(PartitionStats {
            allocated_cpus,
            allocated_memory_mb,
            allocated_gpus,
            running_jobs: self.count_top_level_jobs_in_partition(partition, JobState::Running)?,
            pending_jobs: self.count_top_level_jobs_in_partition(partition, JobState::Pending)?,
            used_gpu_ids: self.partition_gpu_ids_in_use(partition)?,
        })
    }

    fn partition_usage_totals(&self, partition: &str) -> Result<(u32, u64, u32)> {
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(SUM(requested_cpus * requested_tasks), 0), COALESCE(SUM(requested_memory_mb), 0),
                    COALESCE(SUM(requested_gpus), 0)
             FROM jobs
             WHERE state = 'RUNNING' AND parent_job_id IS NULL AND partition = ?1",
        )?;
        stmt.query_row([partition], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(Into::into)
    }

    fn partition_gpu_ids_in_use(&self, partition: &str) -> Result<Vec<u32>> {
        let mut stmt = self.conn.prepare(
            "SELECT assigned_gpus FROM jobs
             WHERE state = 'RUNNING' AND parent_job_id IS NULL AND partition = ?1 AND assigned_gpus <> ''",
        )?;
        let rows = stmt.query_map([partition], |row| row.get::<_, String>(0))?;
        let mut ids = Vec::new();
        for row in rows {
            ids.extend(parse_gpu_ids(&row?)?);
        }
        ids.sort_unstable();
        Ok(ids)
    }

    fn count_top_level_jobs_in_partition(&self, partition: &str, state: JobState) -> Result<usize> {
        let count = self.conn.query_row(
            "SELECT COUNT(*) FROM jobs WHERE partition = ?1 AND state = ?2 AND parent_job_id IS NULL",
            params![partition, state.as_str()],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(count as usize)
    }
}
