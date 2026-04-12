use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

use crate::app::config::AppConfig;
use crate::app::error::Result;
use crate::model::job::JobRecord;
use crate::runtime::cgroup::setup_job_cgroup;
use crate::runtime::cpu::{apply_cpu_affinity, resolve_cpu_bind_ids};
use crate::runtime::foreground::ForegroundExecutionOptions;
use crate::runtime::foreground_io::{ForegroundIoState, configure_foreground_stdio};
use crate::runtime::launch::{LaunchCommand, build_multitask_launcher};
use crate::runtime::slurm_env::apply_slurm_env;

pub(crate) struct ForegroundLaunch {
    pub(crate) child: std::process::Child,
    pub(crate) pid: i32,
    pub(crate) pgid: i32,
    pub(crate) io_state: ForegroundIoState,
}

pub(crate) fn launch_foreground_command(
    config: &AppConfig,
    job: &JobRecord,
    command: &[String],
    options: ForegroundExecutionOptions<'_>,
) -> Result<ForegroundLaunch> {
    let launcher_script = build_multitask_launcher(
        LaunchCommand::Command(command),
        job.requested_tasks.max(1),
        options.io.label_output,
    );
    let mut command_builder = Command::new("/bin/bash");
    command_builder.arg("-lc").arg(launcher_script);
    command_builder.current_dir(&job.cwd);
    command_builder.stdin(Stdio::inherit());
    let io_state = configure_foreground_stdio(
        &mut command_builder,
        &job.cwd,
        options.io.stdout_path,
        options.io.stderr_path,
        false,
        options.io.unbuffered,
    )?;
    apply_slurm_env(&mut command_builder, config, job);
    let cpu_ids = resolve_cpu_bind_ids(
        options.cpu_bind.or(job.cpu_bind.as_deref()),
        config.total_cpus,
        job.requested_cpus
            .saturating_mul(job.requested_tasks)
            .max(1),
    )?;
    unsafe {
        command_builder.pre_exec(|| {
            nix::unistd::setsid().map_err(std::io::Error::other)?;
            Ok(())
        });
    }
    if let Some(cpu_ids) = cpu_ids {
        unsafe {
            command_builder.pre_exec(move || {
                apply_cpu_affinity(&cpu_ids).map_err(std::io::Error::other)?;
                Ok(())
            });
        }
    }
    let child = command_builder.spawn()?;
    let pid = child.id() as i32;
    Ok(ForegroundLaunch {
        child,
        pid,
        pgid: pid,
        io_state,
    })
}

pub(crate) fn setup_local_cgroup(
    config: &AppConfig,
    job_id: i64,
    job: &JobRecord,
    pid: i32,
) -> Result<Option<std::path::PathBuf>> {
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
