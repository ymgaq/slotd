use crate::app::config::AppConfig;
use crate::app::error::{Result, SlotdError};
use crate::command::args::ScontrolArgs;
use crate::command::helpers::{load_job, validate_constraint};
use crate::proto::ipc::{Request, Response, send_request};
use crate::submit::sbatch::parse_time_limit_secs;

pub(super) fn run_update_job(config: AppConfig, args: ScontrolArgs) -> Result<()> {
    let mut name = None;
    let mut partition = None;
    let mut time_limit_secs = None;
    let mut priority = None;
    for update in &args.updates {
        let Some((key, value)) = update.split_once('=') else {
            return Err(SlotdError::from(format!(
                "invalid update expression: {update}"
            )));
        };
        match key.to_ascii_lowercase().as_str() {
            "jobname" | "name" => name = Some(value.to_string()),
            "partition" => partition = Some(value.to_string()),
            "timelimit" | "time" => time_limit_secs = Some(parse_time_limit_secs(value)?),
            "priority" => {
                priority = Some(
                    value
                        .parse::<i32>()
                        .map_err(|_| SlotdError::from(format!("invalid priority: {value}")))?,
                )
            }
            other => return Err(SlotdError::from(format!("unsupported update key: {other}"))),
        }
    }
    if let Some(partition) = partition.as_deref() {
        let job = load_job(&config, args.job_id)?;
        if let Some(constraint) = job.constraint.as_deref() {
            validate_constraint(&config, constraint, partition)?;
        }
    }
    match send_request(
        &config,
        &Request::UpdateJob {
            job_id: args.job_id,
            name,
            partition,
            time_limit_secs,
            priority,
        },
    )? {
        Response::Submitted { .. } => Ok(()),
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response to scontrol update: {other:?}"
        ))),
    }
}
