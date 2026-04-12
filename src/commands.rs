use crate::cli::{
    SacctArgs, ScancelArgs, ScontrolArgs, SinfoArgs, SqueueArgs, build_sinfo_node_rows,
    estimate_start_times, filter_partitions, load_job, parse_states, print_scontrol_job,
    resolve_job_reference, sort_squeue_jobs, validate_constraint,
};
use crate::config::AppConfig;
use crate::error::{Result, SlotdError};
use crate::ipc::{Request, Response, send_request};
use crate::output::{
    SqueueField, parse_sacct_fields, parse_sinfo_fields, parse_squeue_fields, print_sacct_jobs,
    print_sacct_jobs_delimited, print_sinfo, print_sinfo_nodes, print_squeue_jobs,
    print_squeue_jobs_with_options, print_squeue_jobs_with_start_times,
};
use crate::signals::parse_signal_name;
use crate::time::parse_time_filter;
use crate::job::JobState;
use crate::sbatch::parse_time_limit_secs;

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

pub(crate) fn run_scontrol(config: AppConfig, args: ScontrolArgs) -> Result<()> {
    if !args.entity.eq_ignore_ascii_case("job") {
        return Err(SlotdError::from(
            "supported syntax: scontrol <action> job <job_id>",
        ));
    }

    if args.action.eq_ignore_ascii_case("hold") {
        return match send_request(
            &config,
            &Request::HoldJob {
                job_id: args.job_id,
            },
        )? {
            Response::Submitted { .. } => Ok(()),
            Response::Error { message } => Err(SlotdError::from(message)),
            other => Err(SlotdError::from(format!(
                "unexpected response to scontrol hold: {other:?}"
            ))),
        };
    }

    if args.action.eq_ignore_ascii_case("release") {
        return match send_request(
            &config,
            &Request::ReleaseJob {
                job_id: args.job_id,
            },
        )? {
            Response::Submitted { .. } => Ok(()),
            Response::Error { message } => Err(SlotdError::from(message)),
            other => Err(SlotdError::from(format!(
                "unexpected response to scontrol release: {other:?}"
            ))),
        };
    }

    if args.action.eq_ignore_ascii_case("update") {
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
        return match send_request(
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
        };
    }

    if !args.action.eq_ignore_ascii_case("show") {
        return Err(SlotdError::from(
            "supported syntax: scontrol show|hold|release|update job <job_id>; update keys: JobName, Partition, TimeLimit, Priority",
        ));
    }

    match send_request(
        &config,
        &Request::GetJob {
            job_id: args.job_id,
        },
    )? {
        Response::Job { job } => {
            let Some(job) = *job else {
                return Err(SlotdError::from(format!("job {} not found", args.job_id)));
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

pub(crate) fn run_sinfo(config: AppConfig, args: SinfoArgs) -> Result<()> {
    let fields = parse_sinfo_fields(args.format.as_deref(), args.long).map_err(SlotdError::from)?;
    match send_request(&config, &Request::NodeInfo)? {
        Response::NodeInfo { info } => {
            let partitions = filter_partitions(info.partitions, args.partitions.as_deref());
            if args.node {
                let rows = build_sinfo_node_rows(&config, &partitions);
                print_sinfo_nodes(&rows, &fields, args.noheader);
            } else {
                print_sinfo(&config, &partitions, &fields, args.noheader);
            }
            Ok(())
        }
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response to sinfo: {other:?}"
        ))),
    }
}
