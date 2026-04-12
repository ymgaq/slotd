use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::app::config::AppConfig;
use crate::app::error::{Result, SlotdError};
use crate::model::job::{JobRecord, JobState};
use crate::proto::ipc::{Request, Response, send_request};
use crate::runtime::cgroup::{cgroup_oomed, cleanup_cgroup, setup_job_cgroup};
use crate::runtime::cpu::{apply_cpu_affinity, resolve_cpu_bind_ids};
use crate::runtime::foreground_io::{ForegroundIoOptions, configure_foreground_stdio};
use crate::runtime::launch::{LaunchCommand, build_multitask_launcher, shell_join};
use crate::runtime::slurm_env::apply_slurm_env;
use crate::runtime::terminal::{exit_signal, terminal_state_with_reasons};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ForegroundExecutionOptions<'a> {
    pub(crate) record_step: bool,
    pub(crate) cpu_bind: Option<&'a str>,
    pub(crate) io: ForegroundIoOptions<'a>,
}

pub(crate) fn run_foreground_allocation(
    config: &AppConfig,
    job: &JobRecord,
    command: &[String],
) -> Result<()> {
    run_foreground_allocation_with_mode(config, job, command, ForegroundExecutionOptions::default())
}

pub(crate) fn run_foreground_allocation_with_mode(
    config: &AppConfig,
    job: &JobRecord,
    command: &[String],
    options: ForegroundExecutionOptions<'_>,
) -> Result<()> {
    let step_record = if options.record_step {
        Some(start_step_record(config, job, command)?)
    } else {
        None
    };
    let launcher_script = build_multitask_launcher(
        LaunchCommand::Command(command),
        job.requested_tasks,
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
    apply_slurm_env(
        &mut command_builder,
        config,
        step_record.as_ref().unwrap_or(job),
    );
    let cpu_ids = resolve_cpu_bind_ids(
        options.cpu_bind.or(step_record
            .as_ref()
            .and_then(|step| step.cpu_bind.as_deref())),
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
    let mut child = command_builder.spawn()?;
    drop(command_builder);
    let pid = child.id() as i32;
    let pgid = pid;
    let local_cgroup = match setup_local_cgroup(
        config,
        step_record.as_ref().map(|step| step.id).unwrap_or(job.id),
        step_record.as_ref().unwrap_or(job),
        pid,
    ) {
        Ok(path) => path,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    match send_request(
        config,
        &Request::AdoptAllocation {
            job_id: job.id,
            pid,
            pgid,
        },
    )? {
        Response::Submitted { .. } => {}
        Response::Error { message } => return Err(SlotdError::from(message)),
        other => {
            return Err(SlotdError::from(format!(
                "unexpected response while adopting allocation {}: {other:?}",
                job.id
            )));
        }
    }

    let step_job_id = if let Some(step) = &step_record {
        match send_request(
            config,
            &Request::AdoptAllocation {
                job_id: step.id,
                pid,
                pgid,
            },
        )? {
            Response::Submitted { .. } => Some(step.id),
            Response::Error { message } => return Err(SlotdError::from(message)),
            other => {
                return Err(SlotdError::from(format!(
                    "unexpected response while adopting step {}: {other:?}",
                    step.id
                )));
            }
        }
    } else {
        None
    };

    let status = child.wait()?;
    io_state.finish()?;
    let exit_code = status.code();
    let term_signal = exit_signal(&status);
    let (state, reason) = terminal_state_with_reasons(
        exit_code,
        term_signal,
        cgroup_oomed(local_cgroup.as_deref()),
        "",
        "NonZeroExitCode",
        "NonZeroExitCode",
        "NonZeroExitCode",
    );
    match send_request(
        config,
        &Request::FinishAllocation {
            job_id: job.id,
            state,
            exit_code,
            term_signal,
            state_reason: Some(reason.to_string()),
        },
    )? {
        Response::Submitted { .. } => {}
        Response::Error { message } => return Err(SlotdError::from(message)),
        other => {
            return Err(SlotdError::from(format!(
                "unexpected response while finishing allocation {}: {other:?}",
                job.id
            )));
        }
    }

    if let Some(step_job_id) = step_job_id {
        match send_request(
            config,
            &Request::FinishAllocation {
                job_id: step_job_id,
                state,
                exit_code,
                term_signal,
                state_reason: Some(reason.to_string()),
            },
        )? {
            Response::Submitted { .. } => {}
            Response::Error { message } => return Err(SlotdError::from(message)),
            other => {
                return Err(SlotdError::from(format!(
                    "unexpected response while finishing step {}: {other:?}",
                    step_job_id
                )));
            }
        }
    }
    cleanup_cgroup(local_cgroup.as_deref());

    match state {
        JobState::Completed => Ok(()),
        _ => Err(SlotdError::Exit(exit_code.unwrap_or(1))),
    }
}

pub(crate) fn run_foreground_step(
    config: &AppConfig,
    job: &JobRecord,
    command: &[String],
    options: ForegroundExecutionOptions<'_>,
) -> Result<()> {
    let step = start_step_record(config, job, command)?;
    let launcher_script = build_multitask_launcher(
        LaunchCommand::Command(command),
        step.requested_tasks.max(1),
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
    apply_slurm_env(&mut command_builder, config, &step);
    let cpu_ids = resolve_cpu_bind_ids(
        options.cpu_bind.or(step.cpu_bind.as_deref()),
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
    let mut child = command_builder.spawn()?;
    drop(command_builder);
    let pid = child.id() as i32;
    let pgid = pid;
    let local_cgroup = match setup_local_cgroup(config, step.id, &step, pid) {
        Ok(path) => path,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    match send_request(
        config,
        &Request::AdoptAllocation {
            job_id: step.id,
            pid,
            pgid,
        },
    )? {
        Response::Submitted { .. } => {}
        Response::Error { message } => return Err(SlotdError::from(message)),
        other => {
            return Err(SlotdError::from(format!(
                "unexpected response while adopting step {}: {other:?}",
                step.id
            )));
        }
    }
    let status = child.wait()?;
    io_state.finish()?;
    let exit_code = status.code();
    let term_signal = exit_signal(&status);
    let (state, reason) = terminal_state_with_reasons(
        exit_code,
        term_signal,
        cgroup_oomed(local_cgroup.as_deref()),
        "",
        "NonZeroExitCode",
        "NonZeroExitCode",
        "NonZeroExitCode",
    );
    match send_request(
        config,
        &Request::FinishAllocation {
            job_id: step.id,
            state,
            exit_code,
            term_signal,
            state_reason: Some(reason.to_string()),
        },
    )? {
        Response::Submitted { .. } => {}
        Response::Error { message } => return Err(SlotdError::from(message)),
        other => {
            return Err(SlotdError::from(format!(
                "unexpected response while finishing step {}: {other:?}",
                step.id
            )));
        }
    }
    cleanup_cgroup(local_cgroup.as_deref());
    match status.code() {
        Some(0) => Ok(()),
        Some(code) => Err(SlotdError::Exit(code)),
        None => Err(SlotdError::Exit(1)),
    }
}

fn start_step_record(
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

fn setup_local_cgroup(
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

fn command_basename(command: &str) -> String {
    Path::new(command)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(command)
        .to_string()
}

fn current_user_name() -> String {
    std::env::var("USER").unwrap_or_else(|_| "unknown".to_string())
}
