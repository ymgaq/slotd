use crate::app::config::AppConfig;
use crate::app::error::{Result, SlotdError};
use crate::command::helpers::print_scontrol_job;
use crate::proto::ipc::{Request, Response, send_request};

pub(super) fn run_show_job(config: AppConfig, job_id: i64) -> Result<()> {
    match send_request(&config, &Request::GetJob { job_id })? {
        Response::Job { job } => {
            let Some(job) = *job else {
                return Err(SlotdError::from(format!("job {job_id} not found")));
            };
            let steps = match send_request(
                &config,
                &Request::ListSteps {
                    parent_job_id: job.id,
                },
            )? {
                Response::Jobs { jobs } => jobs,
                Response::Error { message } => return Err(SlotdError::from(message)),
                other => {
                    return Err(SlotdError::from(format!(
                        "unexpected response to scontrol step listing: {other:?}"
                    )));
                }
            };
            print_scontrol_job(&config, &job, &steps);
            Ok(())
        }
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response to scontrol: {other:?}"
        ))),
    }
}
