mod batch;
mod interactive;
mod wait;

use crate::app::config::AppConfig;
use crate::app::error::{Result, SlotdError};
use crate::command::args::{SallocArgs, SbatchArgs};
use crate::command::helpers::{load_sbatch_env_overrides, merge_batch_directives};
use crate::model::job::{OpenMode, SubmitRequest};
use crate::proto::ipc::{Request, Response, send_request};
use crate::runtime::foreground::run_foreground_allocation;
use crate::runtime::launch::shell_join;
use crate::util::env::resolve_export_env;
use crate::util::signals::parse_warning_signal;
use crate::util::time::parse_begin_time;

pub(crate) use interactive::run_srun;

pub(crate) fn run_sbatch(config: AppConfig, args: SbatchArgs) -> Result<()> {
    let (script_name, script_body, directives, command_override) =
        batch::resolve_batch_source(&args)?;
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
        exclusive: args.exclusive || env_overrides.exclusive || defaults.exclusive,
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
        requeue: args.requeue || env_overrides.requeue || defaults.requeue,
    };

    match send_request(&config, &Request::SubmitBatch(request))? {
        Response::Submitted { job_id } => {
            if args.parsable {
                println!("{job_id}");
            } else {
                println!("Submitted batch job {job_id}");
            }
            if args.wait {
                wait::wait_for_submission_completion(&config, job_id)
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

pub(crate) fn run_salloc(config: AppConfig, args: SallocArgs) -> Result<()> {
    let resolved = args.resources.resolve(&config, None)?;
    let command = if args.command.is_empty() {
        vec![std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())]
    } else {
        args.command
    };
    let command_name =
        interactive::command_basename(command.first().map(String::as_str).unwrap_or("salloc"));

    let request = SubmitRequest {
        name: resolved.job_name.or_else(|| Some("salloc".to_string())),
        user_name: current_user_name(),
        partition: resolved.partition,
        cwd: resolved.cwd.clone(),
        script_name: command_name,
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
    let job = wait::wait_for_job_running(&config, job_id)?;
    run_foreground_allocation(&config, &job, &command)
}

fn current_user_name() -> String {
    std::env::var("USER").unwrap_or_else(|_| "unknown".to_string())
}
