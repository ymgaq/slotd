use rusqlite::params;

use crate::app::error::Result;
use crate::model::job::{JobState, PartitionInfo};
use crate::store::support::{parse_gpu_ids, partition_gres_used, partition_state};

use super::Store;

impl Store {
    pub(super) fn partition_info(&self, partition: &str) -> Result<PartitionInfo> {
        let (allocated_cpus, allocated_memory_mb, allocated_gpus) =
            self.running_usage_for_partition(partition)?;
        let running_jobs = self.count_jobs_by_partition_and_state(partition, JobState::Running)?;
        let pending_jobs = self.count_jobs_by_partition_and_state(partition, JobState::Pending)?;
        let is_gpu_partition = self.config.is_gpu_partition(partition);
        let state = partition_state(
            is_gpu_partition,
            allocated_cpus,
            allocated_gpus,
            self.config.total_cpus,
            self.config.total_gpus,
        );
        let gres_used = partition_gres_used(
            is_gpu_partition,
            &self.config.gpu_model,
            self.config.total_gpus,
            &self.used_gpu_ids_for_partition(partition)?,
        );
        Ok(PartitionInfo {
            name: partition.to_string(),
            hostname: self.config.hostname.clone(),
            state,
            gres_used,
            features: self.config.format_features(),
            total_cpus: self.config.total_cpus,
            total_memory_mb: self.config.total_memory_mb,
            total_gpus: if self.config.is_gpu_partition(partition) {
                self.config.total_gpus
            } else {
                0
            },
            allocated_cpus,
            allocated_memory_mb,
            allocated_gpus,
            running_jobs,
            pending_jobs,
        })
    }

    fn running_usage_for_partition(&self, partition: &str) -> Result<(u32, u64, u32)> {
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(SUM(requested_cpus * requested_tasks), 0), COALESCE(SUM(requested_memory_mb), 0),
                    COALESCE(SUM(requested_gpus), 0)
             FROM jobs
             WHERE state = 'RUNNING' AND parent_job_id IS NULL AND partition = ?1",
        )?;
        let usage = stmt.query_row([partition], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        Ok(usage)
    }

    fn used_gpu_ids_for_partition(&self, partition: &str) -> Result<Vec<u32>> {
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

    fn count_jobs_by_partition_and_state(&self, partition: &str, state: JobState) -> Result<usize> {
        let count = self.conn.query_row(
            "SELECT COUNT(*) FROM jobs WHERE partition = ?1 AND state = ?2 AND parent_job_id IS NULL",
            params![partition, state.as_str()],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(count as usize)
    }
}
