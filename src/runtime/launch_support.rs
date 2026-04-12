use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

use crate::app::config::AppConfig;
use crate::app::error::Result;
use crate::model::job::JobRecord;
use crate::runtime::cgroup::setup_job_cgroup;
use crate::runtime::cpu::{apply_cpu_affinity, resolve_cpu_bind_ids};

pub(crate) fn configure_detached_launch(
    command: &mut Command,
    cpu_bind: Option<&str>,
    total_cpus: u32,
    requested_slots: u32,
) -> Result<()> {
    let cpu_ids = resolve_cpu_bind_ids(cpu_bind, total_cpus, requested_slots.max(1))?;
    unsafe {
        command.pre_exec(|| {
            nix::unistd::setsid().map_err(std::io::Error::other)?;
            Ok(())
        });
    }
    if let Some(cpu_ids) = cpu_ids {
        unsafe {
            command.pre_exec(move || {
                apply_cpu_affinity(&cpu_ids).map_err(std::io::Error::other)?;
                Ok(())
            });
        }
    }
    Ok(())
}

pub(crate) fn setup_job_cgroup_for_launch(
    config: &AppConfig,
    job_id: i64,
    job: &JobRecord,
    pid: i32,
) -> Result<Option<PathBuf>> {
    setup_job_cgroup(
        config.cgroup_base.as_deref(),
        job_id,
        job.requested_memory_mb,
        job.requested_cpus,
        job.requested_tasks,
        config.total_cpus,
        pid,
    )
}
