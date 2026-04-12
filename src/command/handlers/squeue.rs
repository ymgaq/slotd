use crate::app::config::AppConfig;
use crate::app::error::{Result, SlotdError};
use crate::command::args::SqueueArgs;
use crate::command::helpers::{estimate_start_times, parse_states, sort_squeue_jobs};
use crate::format::{
    SqueueField, parse_squeue_fields, print_squeue_jobs, print_squeue_jobs_with_options,
    print_squeue_jobs_with_start_times,
};
use crate::model::job::JobState;
use crate::proto::ipc::{Request, Response, send_request};

pub(crate) fn run_squeue(config: AppConfig, args: SqueueArgs) -> Result<()> {
    let fields = if args.start && args.format.is_none() {
        vec![
            SqueueField::JobId,
            SqueueField::Partition,
            SqueueField::Name,
            SqueueField::User,
            SqueueField::StateCompact,
            SqueueField::StartTime,
            SqueueField::NodeListReason,
        ]
    } else {
        parse_squeue_fields(args.format.as_deref(), args.long).map_err(SlotdError::from)?
    };
    let state_filter = if let Some(values) = args.states {
        Some(parse_states(values)?)
    } else if args.all {
        None
    } else {
        Some(vec![JobState::Pending, JobState::Running])
    };

    match send_request(
        &config,
        &Request::ListJobs {
            states: state_filter,
            ids: args.jobs,
            user_name: args.user,
            partitions: args.partitions,
        },
    )? {
        Response::Jobs { jobs } => {
            let jobs = sort_squeue_jobs(jobs, args.sort.as_deref());
            if args.start {
                let start_times = estimate_start_times(&config, &jobs);
                print_squeue_jobs_with_start_times(
                    &config,
                    &jobs,
                    &fields,
                    &start_times,
                    args.noheader,
                    args.array,
                );
            } else if args.array {
                print_squeue_jobs_with_options(&config, &jobs, &fields, args.noheader, true);
            } else {
                print_squeue_jobs(&config, &jobs, &fields, args.noheader);
            }
            Ok(())
        }
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response to squeue: {other:?}"
        ))),
    }
}
