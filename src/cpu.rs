use nix::sched::CpuSet;
use nix::unistd::Pid;

use crate::error::{Result, SlotdError};

pub(crate) fn resolve_cpu_bind_ids(
    value: Option<&str>,
    total_cpus: u32,
    requested_cpus: u32,
) -> Result<Option<Vec<usize>>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let normalized = value.to_ascii_lowercase();
    if normalized == "none" {
        return Ok(None);
    }
    if normalized == "cores" {
        let limit = requested_cpus.min(total_cpus).max(1);
        return Ok(Some((0..limit as usize).collect()));
    }
    if let Some(list) = normalized.strip_prefix("map_cpu:") {
        let mut cpus = Vec::new();
        for part in list.split(',').map(str::trim).filter(|part| !part.is_empty()) {
            let cpu = part
                .parse::<usize>()
                .map_err(|_| SlotdError::from(format!("invalid cpu-bind cpu id: {part}")))?;
            if cpu >= total_cpus as usize {
                return Err(SlotdError::from(format!(
                    "cpu-bind cpu id {cpu} exceeds available CPUs"
                )));
            }
            cpus.push(cpu);
        }
        if cpus.is_empty() {
            return Err(SlotdError::from(
                "cpu-bind map_cpu requires at least one CPU id",
            ));
        }
        cpus.sort_unstable();
        cpus.dedup();
        return Ok(Some(cpus));
    }
    Err(SlotdError::from(format!(
        "unsupported cpu-bind value: {value}; supported: none, cores, map_cpu:<ids>"
    )))
}

pub(crate) fn apply_cpu_affinity(cpu_ids: &[usize]) -> Result<()> {
    let mut cpu_set = CpuSet::new();
    for &cpu_id in cpu_ids {
        cpu_set
            .set(cpu_id)
            .map_err(|error| SlotdError::from(error.to_string()))?;
    }
    nix::sched::sched_setaffinity(Pid::from_raw(0), &cpu_set)
        .map_err(|error| SlotdError::from(error.to_string()))
}
