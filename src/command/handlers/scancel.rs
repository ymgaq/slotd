use crate::app::config::AppConfig;
use crate::app::error::{Result, SlotdError};
use crate::command::cli::{ScancelArgs, resolve_job_reference};
use crate::proto::ipc::{Request, Response, send_request};
use crate::util::signals::parse_signal_name;

pub(crate) fn run_scancel(config: AppConfig, args: ScancelArgs) -> Result<()> {
    let job_id = resolve_job_reference(&config, &args.job_id)?;
    if let Some(signal) = args.signal.as_deref() {
        let signal = parse_signal_name(signal)?;
        return match send_request(&config, &Request::SignalJob { job_id, signal })? {
            Response::Submitted { job_id } => {
                println!("Signaled job {job_id}");
                Ok(())
            }
            Response::Error { message } => Err(SlotdError::from(message)),
            other => Err(SlotdError::from(format!(
                "unexpected response to scancel signal: {other:?}"
            ))),
        };
    }

    match send_request(&config, &Request::Cancel { job_id })? {
        Response::Cancelled { job_id } => {
            println!("Cancelled job {job_id}");
            Ok(())
        }
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response to scancel: {other:?}"
        ))),
    }
}
