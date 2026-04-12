use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Result, SlotdError};

pub fn setup_job_cgroup(
    cgroup_base: Option<&Path>,
    job_id: i64,
    requested_memory_mb: u64,
    requested_cpus: u32,
    requested_tasks: u32,
    total_cpus: u32,
    pid: i32,
) -> Result<Option<PathBuf>> {
    let Some(base) = cgroup_base else {
        return Ok(None);
    };
    let path = base.join(format!("slotd-{job_id}"));
    fs::create_dir_all(&path).map_err(|error| cgroup_error(base, job_id, error))?;

    let memory_bytes = requested_memory_mb.saturating_mul(1024 * 1024);
    fs::write(path.join("memory.max"), memory_bytes.to_string())
        .map_err(|error| cgroup_error(base, job_id, error))?;

    let total_requested_cpus = requested_cpus.saturating_mul(requested_tasks).max(1);
    let quota = 100_000u64
        .saturating_mul(total_requested_cpus as u64)
        .checked_div(total_cpus.max(1) as u64)
        .unwrap_or(100_000)
        .max(1);
    fs::write(path.join("cpu.max"), format!("{quota} 100000"))
        .map_err(|error| cgroup_error(base, job_id, error))?;
    fs::write(path.join("cgroup.procs"), pid.to_string())
        .map_err(|error| cgroup_error(base, job_id, error))?;
    Ok(Some(path))
}

pub fn cgroup_oomed(path: Option<&Path>) -> bool {
    let Some(path) = path else {
        return false;
    };
    let Ok(contents) = fs::read_to_string(path.join("memory.events")) else {
        return false;
    };
    contents.lines().any(|line| {
        let mut parts = line.split_whitespace();
        matches!(parts.next(), Some("oom_kill") | Some("oom"))
            && parts
                .next()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(0)
                > 0
    })
}

pub fn cleanup_cgroup(path: Option<&Path>) {
    let Some(path) = path else {
        return;
    };
    let _ = fs::remove_dir(path);
}

fn cgroup_error(base: &Path, job_id: i64, error: std::io::Error) -> SlotdError {
    SlotdError::from(format!(
        "failed to apply cgroup controls under SLOTD_CGROUP_BASE={} for job {}: {}",
        base.display(),
        job_id,
        error
    ))
}
