use crate::app::config::AppConfig;
use crate::app::error::{Result, SlotdError};
use crate::command::args::SallocArgs;
use crate::model::job::{OpenMode, SubmitRequest};
use crate::proto::ipc::{Request, Response, send_request};
use crate::runtime::foreground::run_foreground_allocation;
use crate::runtime::launch::shell_join;

use super::common::current_user_name;
use super::interactive;
use super::wait;

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
