mod allocation;
mod step;

use std::path::Path;

use crate::app::config::AppConfig;
use crate::app::error::{Result, SlotdError};
use crate::model::job::JobRecord;
use crate::proto::ipc::{Request, Response, send_request};
use crate::runtime::foreground_io::ForegroundIoOptions;
use crate::runtime::launch::shell_join;

pub(crate) use allocation::{run_foreground_allocation, run_foreground_allocation_with_mode};
pub(crate) use step::run_foreground_step;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ForegroundExecutionOptions<'a> {
    pub(crate) record_step: bool,
    pub(crate) cpu_bind: Option<&'a str>,
    pub(crate) io: ForegroundIoOptions<'a>,
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
