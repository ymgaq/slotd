use std::path::PathBuf;

use crate::app::config::AppConfig;
use crate::app::error::{Result, SlotdError};
use crate::command::args::SrunArgs;
use crate::model::job::{JobRecord, JobState, OpenMode, SubmitRequest};
use crate::proto::ipc::{Request, Response, send_request};
use crate::runtime::foreground::{
    ForegroundExecutionOptions, run_foreground_allocation_with_mode, run_foreground_step,
};
use crate::runtime::foreground_io::ForegroundIoOptions;
use crate::runtime::launch::shell_join;

use super::common::current_user_name;
use super::wait;

pub(crate) fn run_srun(config: AppConfig, args: SrunArgs) -> Result<()> {
    if args.pty {
        return Err(SlotdError::from(
            "--pty is not implemented yet; use plain foreground srun for now",
        ));
    }
    if let Some(job) = current_allocation_job(&config)? {
        if args.no_wait && (args.label || args.unbuffered) {
            return Err(SlotdError::from(
                "--label and --unbuffered are not supported with --no-wait",
            ));
        }
        return run_foreground_step(
            &config,
            &job,
            &args.command,
            ForegroundExecutionOptions {
                record_step: true,
                cpu_bind: args.cpu_bind.as_deref(),
                io: ForegroundIoOptions {
                    stdout_path: args.output.as_deref(),
                    stderr_path: args.error.as_deref(),
                    label_output: args.label,
                    unbuffered: args.unbuffered,
                },
            },
        );
    }

    let resolved = args.resources.resolve(&config, None)?;
    let command_name = args
        .command
        .first()
        .map(|value| command_basename(value))
        .unwrap_or_else(|| "srun".to_string());
    let command_override = shell_join(&args.command);

    if !args.no_wait {
        return run_interactive_srun(
            &config,
            InteractiveRunSpec {
                name: resolved.job_name.or(Some(command_name)),
                partition: resolved.partition,
                cwd: resolved.cwd,
                requested_cpus: resolved.requested_cpus,
                requested_tasks: resolved.requested_tasks,
                requested_memory_mb: resolved.requested_memory_mb,
                requested_gpus: resolved.requested_gpus,
                time_limit_secs: resolved.time_limit_secs,
                constraint: resolved.constraint,
                cpu_bind: args.cpu_bind,
                label_output: args.label,
                unbuffered: args.unbuffered,
                immediate: args.immediate,
                stdout_path: args.output,
                stderr_path: args.error,
                command: args.command,
            },
        );
    }
    if args.label || args.unbuffered {
        return Err(SlotdError::from(
            "--label and --unbuffered are not supported with --no-wait",
        ));
    }

    let request = SubmitRequest {
        name: resolved.job_name.or_else(|| Some(command_name.clone())),
        user_name: current_user_name(),
        partition: resolved.partition,
        cwd: resolved.cwd,
        script_name: command_name,
        script_body: format!("#!/usr/bin/env bash\nexec {}\n", command_override),
        command_override: Some(command_override),
        requested_cpus: resolved.requested_cpus,
        requested_tasks: resolved.requested_tasks,
        requested_memory_mb: resolved.requested_memory_mb,
        requested_gpus: resolved.requested_gpus,
        allocation_only: false,
        dependency: None,
        array_spec: None,
        time_limit_secs: resolved.time_limit_secs,
        begin_time: None,
        exclusive: false,
        stdout_path: args
            .output
            .clone()
            .map(|path| path.to_string_lossy().to_string()),
        stderr_path: args
            .error
            .clone()
            .map(|path| path.to_string_lossy().to_string()),
        constraint: resolved.constraint,
        cpu_bind: args.cpu_bind,
        export_env: Vec::new(),
        open_mode: OpenMode::Truncate,
        warning_signal: None,
        requeue: false,
    };

    let job_id = match send_request(
        &config,
        &Request::SubmitRun {
            request,
            immediate: args.immediate,
        },
    )? {
        Response::Submitted { job_id } => job_id,
        Response::Error { message } => return Err(SlotdError::from(message)),
        other => {
            return Err(SlotdError::from(format!(
                "unexpected response to srun: {other:?}"
            )));
        }
    };

    if args.no_wait {
        println!("Submitted run job {job_id}");
    }
    Ok(())
}

struct InteractiveRunSpec {
    name: Option<String>,
    partition: String,
    cwd: String,
    requested_cpus: u32,
    requested_tasks: u32,
    requested_memory_mb: u64,
    requested_gpus: u32,
    time_limit_secs: Option<u64>,
    constraint: Option<String>,
    cpu_bind: Option<String>,
    label_output: bool,
    unbuffered: bool,
    immediate: bool,
    stdout_path: Option<PathBuf>,
    stderr_path: Option<PathBuf>,
    command: Vec<String>,
}

fn run_interactive_srun(config: &AppConfig, spec: InteractiveRunSpec) -> Result<()> {
    let cpu_bind = spec.cpu_bind.clone();
    let request = SubmitRequest {
        name: spec.name.or_else(|| Some("srun".to_string())),
        user_name: current_user_name(),
        partition: spec.partition,
        cwd: spec.cwd,
        script_name: "srun".to_string(),
        script_body: String::new(),
        command_override: Some(shell_join(&spec.command)),
        requested_cpus: spec.requested_cpus,
        requested_tasks: spec.requested_tasks,
        requested_memory_mb: spec.requested_memory_mb,
        requested_gpus: spec.requested_gpus,
        allocation_only: true,
        dependency: None,
        array_spec: None,
        time_limit_secs: spec.time_limit_secs,
        begin_time: None,
        exclusive: false,
        stdout_path: None,
        stderr_path: None,
        constraint: spec.constraint,
        cpu_bind,
        export_env: Vec::new(),
        open_mode: OpenMode::Truncate,
        warning_signal: None,
        requeue: false,
    };

    let job_id = match send_request(
        config,
        &Request::SubmitAlloc {
            request,
            immediate: spec.immediate,
        },
    )? {
        Response::Submitted { job_id } => job_id,
        Response::Error { message } => {
            let message =
                if message == "resources are not currently available for --immediate salloc" {
                    "resources are not currently available for --immediate srun".to_string()
                } else {
                    message
                };
            return Err(SlotdError::from(message));
        }
        other => {
            return Err(SlotdError::from(format!(
                "unexpected response to interactive srun: {other:?}"
            )));
        }
    };

    let job = wait::wait_for_job_running(config, job_id)?;
    run_foreground_allocation_with_mode(
        config,
        &job,
        &spec.command,
        ForegroundExecutionOptions {
            record_step: true,
            cpu_bind: spec.cpu_bind.as_deref(),
            io: ForegroundIoOptions {
                stdout_path: spec.stdout_path.as_deref(),
                stderr_path: spec.stderr_path.as_deref(),
                label_output: spec.label_output,
                unbuffered: spec.unbuffered,
            },
        },
    )
}

fn current_allocation_job(config: &AppConfig) -> Result<Option<JobRecord>> {
    let Ok(value) = std::env::var("SLURM_JOB_ID") else {
        return Ok(None);
    };
    let Ok(job_id) = value.parse::<i64>() else {
        return Ok(None);
    };

    match send_request(config, &Request::GetJob { job_id })? {
        Response::Job { job } => {
            Ok((*job).filter(|job| job.state == JobState::Running && job.allocation_only))
        }
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response while loading current allocation: {other:?}"
        ))),
    }
}

pub(super) fn command_basename(command: &str) -> String {
    PathBuf::from(command)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("srun")
        .to_string()
}
