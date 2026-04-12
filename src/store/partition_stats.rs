use rusqlite::params;

use super::Store;
use crate::app::error::Result;
use crate::model::job::JobState;

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
            self.running_usage_totals(Some(partition))?;
        Ok(PartitionStats {
            allocated_cpus,
            allocated_memory_mb,
            allocated_gpus,
            running_jobs: self.count_top_level_jobs_in_partition(partition, JobState::Running)?,
            pending_jobs: self.count_top_level_jobs_in_partition(partition, JobState::Pending)?,
            used_gpu_ids: {
                let mut ids = self.gpu_ids_in_use(Some(partition))?;
                ids.sort_unstable();
                ids
            },
        })
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
