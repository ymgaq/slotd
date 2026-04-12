use std::process::{Command, Stdio};

use crate::app::config::AppConfig;
use crate::app::error::Result;
use crate::model::job::JobRecord;
use crate::runtime::foreground::ForegroundExecutionOptions;
use crate::runtime::foreground_io::{ForegroundIoState, configure_foreground_stdio};
use crate::runtime::launch::{LaunchCommand, build_multitask_launcher};
use crate::runtime::launch_support::{configure_detached_launch, setup_job_cgroup_for_launch};
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
    configure_detached_launch(
        &mut command_builder,
        options.cpu_bind.or(job.cpu_bind.as_deref()),
        config.total_cpus,
        job.requested_cpus
            .saturating_mul(job.requested_tasks)
            .max(1),
    )?;
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
    setup_job_cgroup_for_launch(config, job_id, job, pid)
}
