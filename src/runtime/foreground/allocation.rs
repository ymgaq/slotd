use crate::app::config::AppConfig;
use crate::app::error::{Result, SlotdError};
use crate::model::job::{JobRecord, JobState};
use crate::proto::ipc::{Request, Response, send_request};
use crate::runtime::cgroup::{cgroup_oomed, cleanup_cgroup};
use crate::runtime::foreground_launch::{launch_foreground_command, setup_local_cgroup};
use crate::runtime::terminal::{exit_signal, terminal_state_with_reasons};

use super::{ForegroundExecutionOptions, ipc::start_step_record};

pub(crate) fn run_foreground_allocation(
    config: &AppConfig,
    job: &JobRecord,
    command: &[String],
) -> Result<()> {
    run_foreground_allocation_with_mode(config, job, command, ForegroundExecutionOptions::default())
}

pub(crate) fn run_foreground_allocation_with_mode(
    config: &AppConfig,
    job: &JobRecord,
    command: &[String],
    options: ForegroundExecutionOptions<'_>,
) -> Result<()> {
    let step_record = if options.record_step {
        Some(start_step_record(config, job, command)?)
    } else {
        None
    };
    let launched = launch_foreground_command(
        config,
        step_record.as_ref().unwrap_or(job),
        command,
        ForegroundExecutionOptions {
            record_step: false,
            cpu_bind: options.cpu_bind.or(step_record
                .as_ref()
                .and_then(|step| step.cpu_bind.as_deref())),
            io: options.io,
        },
    )?;
    let local_cgroup = match setup_local_cgroup(
        config,
        step_record.as_ref().map(|step| step.id).unwrap_or(job.id),
        step_record.as_ref().unwrap_or(job),
        launched.pid,
    ) {
        Ok(path) => path,
        Err(error) => {
            let mut child = launched.child;
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    adopt_foreground_job(config, job.id, launched.pid, launched.pgid, "allocation")?;
    if let Some(step) = &step_record {
        adopt_foreground_job(config, step.id, launched.pid, launched.pgid, "step")?;
    }

    let mut child = launched.child;
    let status = child.wait()?;
    launched.io_state.finish()?;
    let exit_code = status.code();
    let term_signal = exit_signal(&status);
    let (state, reason) = terminal_state_with_reasons(
        exit_code,
        term_signal,
        cgroup_oomed(local_cgroup.as_deref()),
        "",
        "NonZeroExitCode",
        "NonZeroExitCode",
        "NonZeroExitCode",
    );
    finish_foreground_job(
        config,
        job.id,
        state,
        exit_code,
        term_signal,
        reason,
        "allocation",
    )?;
    if let Some(step) = &step_record {
        finish_foreground_job(
            config,
            step.id,
            state,
            exit_code,
            term_signal,
            reason,
            "step",
        )?;
    }
    cleanup_cgroup(local_cgroup.as_deref());

    match state {
        JobState::Completed => Ok(()),
        _ => Err(SlotdError::Exit(exit_code.unwrap_or(1))),
    }
}

fn adopt_foreground_job(
    config: &AppConfig,
    job_id: i64,
    pid: i32,
    pgid: i32,
    label: &str,
) -> Result<()> {
    match send_request(config, &Request::AdoptAllocation { job_id, pid, pgid })? {
        Response::Submitted { .. } => Ok(()),
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response while adopting {label} {job_id}: {other:?}"
        ))),
    }
}

fn finish_foreground_job(
    config: &AppConfig,
    job_id: i64,
    state: JobState,
    exit_code: Option<i32>,
    term_signal: Option<i32>,
    reason: &str,
    label: &str,
) -> Result<()> {
    match send_request(
        config,
        &Request::FinishAllocation {
            job_id,
            state,
            exit_code,
            term_signal,
            state_reason: Some(reason.to_string()),
        },
    )? {
        Response::Submitted { .. } => Ok(()),
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response while finishing {label} {job_id}: {other:?}"
        ))),
    }
}
