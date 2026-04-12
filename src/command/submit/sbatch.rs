use crate::app::config::AppConfig;
use crate::app::error::{Result, SlotdError};
use crate::command::args::SbatchArgs;
use crate::command::helpers::{load_sbatch_env_overrides, merge_batch_directives};
use crate::model::job::{OpenMode, SubmitRequest};
use crate::proto::ipc::{Request, Response, send_request};
use crate::util::env::resolve_export_env;
use crate::util::signals::parse_warning_signal;
use crate::util::time::parse_begin_time;

use super::batch;
use super::common::current_user_name;
use super::wait;

pub(crate) fn run_sbatch(config: AppConfig, args: SbatchArgs) -> Result<()> {
    let request = build_sbatch_request(&config, &args)?;
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

fn build_sbatch_request(config: &AppConfig, args: &SbatchArgs) -> Result<SubmitRequest> {
    let (script_name, script_body, directives, command_override) =
        batch::resolve_batch_source(args)?;
    let env_overrides = load_sbatch_env_overrides();
    let defaults = merge_batch_directives(&directives, &env_overrides.directives);
    let resolved = args.resources.resolve(config, Some(&defaults))?;
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

    Ok(SubmitRequest {
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
        dependency: args.dependency.clone().or(defaults.dependency),
        array_spec: args.array.clone().or(defaults.array_spec),
        time_limit_secs: resolved.time_limit_secs,
        begin_time,
        exclusive: args.exclusive || env_overrides.exclusive || defaults.exclusive,
        stdout_path: args
            .output
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
            .or(defaults.output_path),
        stderr_path: args
            .error
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
            .or(defaults.error_path),
        constraint: resolved.constraint,
        cpu_bind: None,
        export_env,
        open_mode,
        warning_signal,
        requeue: args.requeue || env_overrides.requeue || defaults.requeue,
    })
}
