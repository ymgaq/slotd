use crate::app::error::Result;
use crate::model::job::PartitionInfo;
use crate::store::support::{partition_gres_used, partition_state};

use super::Store;

impl Store {
    pub(super) fn partition_info(&self, partition: &str) -> Result<PartitionInfo> {
        let stats = self.load_partition_stats(partition)?;
        let is_gpu_partition = self.config.is_gpu_partition(partition);
        let state = partition_state(
            is_gpu_partition,
            stats.allocated_cpus,
            stats.allocated_gpus,
            self.config.total_cpus,
            self.config.total_gpus,
        );
        let gres_used = partition_gres_used(
            is_gpu_partition,
            &self.config.gpu_model,
            self.config.total_gpus,
            &stats.used_gpu_ids,
        );
        Ok(PartitionInfo {
            name: partition.to_string(),
            hostname: self.config.hostname.clone(),
            state,
            gres_used,
            features: self.config.format_features(partition),
            total_cpus: self.config.total_cpus,
            total_memory_mb: self.config.total_memory_mb,
            total_gpus: if self.config.is_gpu_partition(partition) {
                self.config.total_gpus
            } else {
                0
            },
            allocated_cpus: stats.allocated_cpus,
            allocated_memory_mb: stats.allocated_memory_mb,
            allocated_gpus: stats.allocated_gpus,
            running_jobs: stats.running_jobs,
            pending_jobs: stats.pending_jobs,
        })
    }
}
