mod show;
mod update;

use crate::app::config::AppConfig;
use crate::app::error::{Result, SlotdError};
use crate::command::args::ScontrolArgs;
use crate::proto::ipc::{Request, Response, send_request};

pub(crate) fn run_scontrol(config: AppConfig, args: ScontrolArgs) -> Result<()> {
    if !args.entity.eq_ignore_ascii_case("job") {
        return Err(SlotdError::from(
            "supported syntax: scontrol <action> job <job_id>",
        ));
    }

    if args.action.eq_ignore_ascii_case("hold") {
        return submit_control_request(
            &config,
            Request::HoldJob {
                job_id: args.job_id,
            },
            "scontrol hold",
        );
    }

    if args.action.eq_ignore_ascii_case("release") {
        return submit_control_request(
            &config,
            Request::ReleaseJob {
                job_id: args.job_id,
            },
            "scontrol release",
        );
    }

    if args.action.eq_ignore_ascii_case("update") {
        return update::run_update_job(config, args);
    }

    if !args.action.eq_ignore_ascii_case("show") {
        return Err(SlotdError::from(
            "supported syntax: scontrol show|hold|release|update job <job_id>; update keys: JobName, Partition, TimeLimit, Priority",
        ));
    }

    show::run_show_job(config, args.job_id)
}

fn submit_control_request(config: &AppConfig, request: Request, action: &str) -> Result<()> {
    match send_request(config, &request)? {
        Response::Submitted { .. } => Ok(()),
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response to {action}: {other:?}"
        ))),
    }
}
