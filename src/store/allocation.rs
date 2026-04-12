use crate::app::error::{Result, SlotdError};
use crate::store::support::parse_gpu_ids;

use super::Store;

impl Store {
    pub fn running_resource_usage(&self) -> Result<(u32, u64, u32)> {
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(SUM(requested_cpus * requested_tasks), 0), COALESCE(SUM(requested_memory_mb), 0),
                    COALESCE(SUM(requested_gpus), 0)
             FROM jobs
             WHERE state = 'RUNNING' AND parent_job_id IS NULL",
        )?;
        let (cpus, mem, gpus) = stmt.query_row([], |row| {
            let cpus: u32 = row.get(0)?;
            let mem: u64 = row.get(1)?;
            let gpus: u32 = row.get(2)?;
            Ok((cpus, mem, gpus))
        })?;
        Ok((cpus, mem, gpus))
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

        let mut used = self.used_gpu_ids()?;
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

    fn used_gpu_ids(&self) -> Result<Vec<u32>> {
        let mut stmt = self.conn.prepare(
            "SELECT assigned_gpus FROM jobs WHERE state = 'RUNNING' AND parent_job_id IS NULL AND assigned_gpus <> ''",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut ids = Vec::new();
        for row in rows {
            ids.extend(parse_gpu_ids(&row?)?);
        }
        Ok(ids)
    }
}
