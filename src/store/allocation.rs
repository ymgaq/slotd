use super::Store;
use crate::app::error::{Result, SlotdError};

impl Store {
    pub fn running_resource_usage(&self) -> Result<(u32, u64, u32)> {
        self.running_usage_totals(None)
    }

    pub fn available_resources(&self) -> Result<(u32, u64, u32)> {
        let (used_cpus, used_memory, used_gpus) = self.running_resource_usage()?;
        Ok((
            self.config.total_cpus.saturating_sub(used_cpus),
            self.config.total_memory_mb.saturating_sub(used_memory),
            self.config.total_gpus.saturating_sub(used_gpus),
        ))
    }

    pub fn allocate_gpu_ids(&self, requested_gpus: u32) -> Result<Vec<u32>> {
        if requested_gpus == 0 {
            return Ok(Vec::new());
        }

        let mut used = self.gpu_ids_in_use(None)?;
        used.sort_unstable();

        let mut assigned = Vec::new();
        for gpu_id in 0..self.config.total_gpus {
            if used.binary_search(&gpu_id).is_err() {
                assigned.push(gpu_id);
                if assigned.len() == requested_gpus as usize {
                    break;
                }
            }
        }

        if assigned.len() == requested_gpus as usize {
            Ok(assigned)
        } else {
            Err(SlotdError::from(
                "not enough free GPU IDs to satisfy the request",
            ))
        }
    }
}
