use std::collections::HashMap;

use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;

use crate::app::config::AppConfig;
use crate::app::error::{Result, SlotdError};
use crate::model::job::JobState;
use crate::runtime::runner::{Runner, RunningJob};
use crate::runtime::runner_lifecycle::{
    terminate_running_job, timed_out_jobs, warning_signal_to_send,
};
use crate::store::Store;

pub(crate) fn enforce_timeouts(
    runner: &mut Runner,
    config: &AppConfig,
    store: &Store,
) -> Result<()> {
    for job_id in timed_out_jobs(store, runner.jobs()) {
        terminate_tracked_job(
            runner.jobs_mut(),
            config,
            store,
            job_id,
            JobState::Timeout,
            "TimeLimit",
        )?;
    }

    let jobs_to_warn = runner
        .jobs()
        .iter()
        .filter_map(|(&job_id, running)| (!running.warning_signal_sent).then_some(job_id))
        .collect::<Vec<_>>();
    for job_id in jobs_to_warn {
        if let Some(signal) = warning_signal_to_send(store, runner.jobs(), job_id)? {
            signal_running_job(runner.jobs(), job_id, signal)?;
            if let Some(running) = runner.jobs_mut().get_mut(&job_id) {
                running.warning_signal_sent = true;
            }
        }
    }

    Ok(())
}

pub(crate) fn cancel_job(
    runner: &mut Runner,
    config: &AppConfig,
    store: &Store,
    job_id: i64,
) -> Result<bool> {
    if store.cancel_pending_job(job_id)? {
        return Ok(true);
    }

    if !runner.jobs().contains_key(&job_id) {
        return Ok(false);
    }

    terminate_tracked_job(
        runner.jobs_mut(),
        config,
        store,
        job_id,
        JobState::Cancelled,
        "CancelledByUser",
    )?;
    Ok(true)
}

pub(crate) fn signal_running_job(
    jobs: &HashMap<i64, RunningJob>,
    job_id: i64,
    signal: i32,
) -> Result<bool> {
    let Some(running) = jobs.get(&job_id) else {
        return Ok(false);
    };
    let signal = Signal::try_from(signal)
        .map_err(|_| SlotdError::from(format!("unsupported signal: {signal}")))?;
    let _ = killpg(Pid::from_raw(running.pgid), signal);
    Ok(true)
}

fn terminate_tracked_job(
    jobs: &mut HashMap<i64, RunningJob>,
    config: &AppConfig,
    store: &Store,
    job_id: i64,
    final_state: JobState,
    final_reason: &str,
) -> Result<()> {
    let Some(running) = jobs.remove(&job_id) else {
        return Ok(());
    };
    terminate_running_job(config, store, job_id, running, final_state, final_reason)
}
