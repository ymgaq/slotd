use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use crate::app::config::AppConfig;
use crate::app::error::{Result, SlotdError};
use crate::command::args::{SallocArgs, SbatchArgs, SrunArgs};
use crate::command::helpers::{load_sbatch_env_overrides, merge_batch_directives};
use crate::model::job::{JobRecord, JobState, OpenMode, SubmitRequest};
use crate::proto::ipc::{Request, Response, send_request};
use crate::runtime::foreground::{
    ForegroundExecutionOptions, run_foreground_allocation, run_foreground_allocation_with_mode,
    run_foreground_step,
};
use crate::runtime::foreground_io::ForegroundIoOptions;
use crate::runtime::launch::shell_join;
use crate::submit::sbatch::parse_directives;
use crate::util::env::resolve_export_env;
use crate::util::signals::parse_warning_signal;
use crate::util::time::parse_begin_time;

pub(crate) fn run_sbatch(config: AppConfig, args: SbatchArgs) -> Result<()> {
    let (script_name, script_body, directives, command_override) = if let Some(wrap) = &args.wrap {
        (
            "wrap".to_string(),
            format!("#!/usr/bin/env bash\n{}\n", wrap),
            crate::submit::sbatch::BatchDirectives::default(),
            Some(wrap.clone()),
        )
    } else {
        let script = args
            .script
            .as_ref()
            .ok_or_else(|| SlotdError::from("script is required"))?;
        let script_body = fs::read_to_string(script)?;
        let directives = parse_directives(&script_body)?;
        let script_name = script
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("script.sh")
            .to_string();
        (script_name, script_body, directives, None)
    };
    let env_overrides = load_sbatch_env_overrides();
    let defaults = merge_batch_directives(&directives, &env_overrides.directives);
    let resolved = args.resources.resolve(&config, Some(&defaults))?;
    let export_env = resolve_export_env(
        args.export.as_deref().or(env_overrides.export.as_deref()),
        args.export_file
            .as_deref()
            .or(env_overrides.export_file.as_deref()),
    )?;
    let open_mode = args
        .open_mode
        .as_deref()
        .or(env_overrides.open_mode.as_deref())
        .unwrap_or("truncate")
        .parse::<OpenMode>()
        .map_err(SlotdError::from)?;
    let warning_signal = args
        .signal
        .as_deref()
        .or(env_overrides.signal.as_deref())
        .map(parse_warning_signal)
        .transpose()?;
    let begin_time = args
        .begin
        .as_deref()
        .or(env_overrides.begin.as_deref())
        .or(defaults.begin.as_deref())
        .map(parse_begin_time)
        .transpose()?;
    let exclusive = args.exclusive || env_overrides.exclusive || defaults.exclusive;
    let requeue = args.requeue || env_overrides.requeue || defaults.requeue;

    let request = SubmitRequest {
        name: resolved.job_name,
        user_name: current_user_name(),
        partition: resolved.partition,
        cwd: resolved.cwd,
        script_name,
        script_body,
        command_override,
        requested_cpus: resolved.requested_cpus,
        requested_tasks: resolved.requested_tasks,
        requested_memory_mb: resolved.requested_memory_mb,
        requested_gpus: resolved.requested_gpus,
        allocation_only: false,
        dependency: args.dependency.or(defaults.dependency),
        array_spec: args.array.or(defaults.array_spec),
        time_limit_secs: resolved.time_limit_secs,
        begin_time,
        exclusive,
        stdout_path: args
            .output
            .map(|path| path.to_string_lossy().to_string())
            .or(defaults.output_path),
        stderr_path: args
            .error
            .map(|path| path.to_string_lossy().to_string())
            .or(defaults.error_path),
        constraint: resolved.constraint,
        cpu_bind: None,
        export_env,
        open_mode,
        warning_signal,
        requeue,
    };

    match send_request(&config, &Request::SubmitBatch(request))? {
        Response::Submitted { job_id } => {
            if args.parsable {
                println!("{job_id}");
            } else {
                println!("Submitted batch job {job_id}");
            }
            if args.wait {
                wait_for_submission_completion(&config, job_id)
            } else {
                Ok(())
            }
        }
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response to sbatch: {other:?}"
        ))),
    }
}

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

    let script_body = format!("#!/usr/bin/env bash\nexec {}\n", command_override);

    let request = SubmitRequest {
        name: resolved.job_name.or_else(|| Some(command_name.clone())),
        user_name: current_user_name(),
        partition: resolved.partition,
        cwd: resolved.cwd,
        script_name: command_name,
        script_body,
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
        return Ok(());
    }

    Ok(())
}

pub(crate) fn run_salloc(config: AppConfig, args: SallocArgs) -> Result<()> {
    let resolved = args.resources.resolve(&config, None)?;
    let command = if args.command.is_empty() {
        vec![std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())]
    } else {
        args.command
    };
    let command_name = command
        .first()
        .map(|value| command_basename(value))
        .unwrap_or_else(|| "salloc".to_string());

    let request = SubmitRequest {
        name: resolved.job_name.or_else(|| Some("salloc".to_string())),
        user_name: current_user_name(),
        partition: resolved.partition,
        cwd: resolved.cwd.clone(),
        script_name: command_name.clone(),
        script_body: String::new(),
        command_override: Some(shell_join(&command)),
        requested_cpus: resolved.requested_cpus,
        requested_tasks: resolved.requested_tasks,
        requested_memory_mb: resolved.requested_memory_mb,
        requested_gpus: resolved.requested_gpus,
        allocation_only: true,
        dependency: None,
        array_spec: None,
        time_limit_secs: resolved.time_limit_secs,
        begin_time: None,
        exclusive: false,
        stdout_path: None,
        stderr_path: None,
        constraint: resolved.constraint,
        cpu_bind: None,
        export_env: Vec::new(),
        open_mode: OpenMode::Truncate,
        warning_signal: None,
        requeue: false,
    };

    let job_id = match send_request(
        &config,
        &Request::SubmitAlloc {
            request,
            immediate: args.immediate,
        },
    )? {
        Response::Submitted { job_id } => job_id,
        Response::Error { message } => return Err(SlotdError::from(message)),
        other => {
            return Err(SlotdError::from(format!(
                "unexpected response to salloc: {other:?}"
            )));
        }
    };

    println!("Granted job allocation {job_id}");
    let job = wait_for_job_running(&config, job_id)?;
    run_foreground_allocation(&config, &job, &command)
}

fn wait_for_job_completion(config: &AppConfig, job_id: i64) -> Result<JobRecord> {
    loop {
        match send_request(config, &Request::GetJob { job_id })? {
            Response::Job { job } => match *job {
                Some(job) if job.state.is_terminal() => return Ok(job),
                Some(_) => thread::sleep(Duration::from_millis(200)),
                None => return Err(SlotdError::from(format!("job {job_id} disappeared"))),
            },
            Response::Error { message } => return Err(SlotdError::from(message)),
            other => {
                return Err(SlotdError::from(format!(
                    "unexpected response while waiting for job {job_id}: {other:?}"
                )));
            }
        }
    }
}

fn wait_for_submission_completion(config: &AppConfig, job_id: i64) -> Result<()> {
    let job = wait_for_job_completion(config, job_id)?;
    let mut jobs = vec![job.clone()];

    if job.array_job_id == Some(job.id) && job.array_task_count.unwrap_or(1) > 1 {
        loop {
            match send_request(
                config,
                &Request::ListAccountingJobs {
                    states: None,
                    ids: None,
                    user_name: None,
                    partitions: None,
                    start_time: None,
                    end_time: None,
                },
            )? {
                Response::Jobs { jobs: all_jobs } => {
                    jobs = all_jobs
                        .into_iter()
                        .filter(|entry| entry.array_job_id == Some(job.id))
                        .collect();
                    if jobs.len() as u32 >= job.array_task_count.unwrap_or(0)
                        && jobs.iter().all(|entry| entry.state.is_terminal())
                    {
                        break;
                    }
                }
                Response::Error { message } => return Err(SlotdError::from(message)),
                other => {
                    return Err(SlotdError::from(format!(
                        "unexpected response while waiting for array job {job_id}: {other:?}"
                    )));
                }
            }
            thread::sleep(Duration::from_millis(200));
        }
    }

    let failing_job = jobs
        .into_iter()
        .find(|entry| entry.state != JobState::Completed);
    if let Some(job) = failing_job {
        return Err(SlotdError::Exit(job.exit_code.unwrap_or(1)));
    }
    Ok(())
}

fn wait_for_job_running(config: &AppConfig, job_id: i64) -> Result<JobRecord> {
    loop {
        match send_request(config, &Request::GetJob { job_id })? {
            Response::Job { job } => match *job {
                Some(job) if job.state == JobState::Running => return Ok(job),
                Some(job) if job.state.is_terminal() => {
                    return Err(SlotdError::from(format!(
                        "allocation {job_id} ended before it became runnable: {}",
                        job.state.as_str()
                    )));
                }
                Some(_) => thread::sleep(Duration::from_millis(200)),
                None => {
                    return Err(SlotdError::from(format!("allocation {job_id} disappeared")));
                }
            },
            Response::Error { message } => return Err(SlotdError::from(message)),
            other => {
                return Err(SlotdError::from(format!(
                    "unexpected response while waiting for allocation {job_id}: {other:?}"
                )));
            }
        }
    }
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

    let job = wait_for_job_running(config, job_id)?;
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

fn command_basename(command: &str) -> String {
    PathBuf::from(command)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("srun")
        .to_string()
}

fn current_user_name() -> String {
    std::env::var("USER").unwrap_or_else(|_| "unknown".to_string())
}
