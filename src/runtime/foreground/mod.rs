mod allocation;
mod step;

use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::app::config::AppConfig;
use crate::app::error::{Result, SlotdError};
use crate::model::job::JobRecord;
use crate::proto::ipc::{Request, Response, send_request};
use crate::runtime::cgroup::setup_job_cgroup;
use crate::runtime::cpu::{apply_cpu_affinity, resolve_cpu_bind_ids};
use crate::runtime::foreground_io::{
    ForegroundIoOptions, ForegroundIoState, configure_foreground_stdio,
};
use crate::runtime::launch::{LaunchCommand, build_multitask_launcher, shell_join};
use crate::runtime::slurm_env::apply_slurm_env;

pub(crate) use allocation::{run_foreground_allocation, run_foreground_allocation_with_mode};
pub(crate) use step::run_foreground_step;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ForegroundExecutionOptions<'a> {
    pub(crate) record_step: bool,
    pub(crate) cpu_bind: Option<&'a str>,
    pub(crate) io: ForegroundIoOptions<'a>,
}

pub(super) struct ForegroundLaunch {
    pub(super) child: std::process::Child,
    pub(super) pid: i32,
    pub(super) pgid: i32,
    pub(super) io_state: ForegroundIoState,
}

pub(super) fn launch_foreground_command(
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

pub(super) fn start_step_record(
    config: &AppConfig,
    parent: &JobRecord,
    command: &[String],
) -> Result<JobRecord> {
    let step_job_id = match send_request(
        config,
        &Request::StartStep {
            parent_job_id: parent.id,
            name: command_basename(&command[0]),
            command: shell_join(command),
            cwd: parent.cwd.clone(),
            user_name: current_user_name(),
        },
    )? {
        Response::Submitted { job_id } => job_id,
        Response::Error { message } => return Err(SlotdError::from(message)),
        other => {
            return Err(SlotdError::from(format!(
                "unexpected response while starting step for job {}: {other:?}",
                parent.id
            )));
        }
    };

    match send_request(
        config,
        &Request::GetJob {
            job_id: step_job_id,
        },
    )? {
        Response::Job { job } => {
            (*job).ok_or_else(|| SlotdError::from(format!("step {step_job_id} disappeared")))
        }
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response while loading step {}: {other:?}",
            step_job_id
        ))),
    }
}

pub(super) fn setup_local_cgroup(
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

pub(super) fn command_basename(command: &str) -> String {
    Path::new(command)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(command)
        .to_string()
}

pub(super) fn current_user_name() -> String {
    std::env::var("USER").unwrap_or_else(|_| "unknown".to_string())
}
