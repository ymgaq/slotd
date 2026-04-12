use crate::app::config::AppConfig;
use crate::app::error::{Result, SlotdError};
use crate::command::args::SacctArgs;
use crate::command::helpers::parse_states;
use crate::format::{parse_sacct_fields, print_sacct_jobs, print_sacct_jobs_delimited};
use crate::proto::ipc::{Request, Response, send_request};
use crate::util::time::parse_time_filter;

pub(crate) fn run_sacct(config: AppConfig, args: SacctArgs) -> Result<()> {
    let state_filter = args.states.map(parse_states).transpose()?;
    let fields = parse_sacct_fields(args.format.as_deref()).map_err(SlotdError::from)?;
    let start_time = args
        .start_time
        .as_deref()
        .map(parse_time_filter)
        .transpose()?;
    let end_time = args
        .end_time
        .as_deref()
        .map(parse_time_filter)
        .transpose()?;

    match send_request(
        &config,
        &Request::ListAccountingJobs {
            states: state_filter,
            ids: args.jobs,
            user_name: args.user,
            partitions: args.partitions,
            start_time,
            end_time,
        },
    )? {
        Response::Jobs { jobs } => {
            if args.parsable2 {
                print_sacct_jobs_delimited(&config, &jobs, &fields, args.noheader, "|");
            } else {
                print_sacct_jobs(&config, &jobs, &fields, args.noheader);
            }
            Ok(())
        }
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response to sacct: {other:?}"
        ))),
    }
}
