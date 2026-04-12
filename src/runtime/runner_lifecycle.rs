use std::collections::HashMap;
use std::thread;
use std::time::Duration;

use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;

use crate::app::config::AppConfig;
use crate::app::error::Result;
use crate::model::job::JobState;
use crate::runtime::cgroup::{cgroup_oomed, cleanup_cgroup};
use crate::runtime::notify::notify_job;
use crate::runtime::runner::{JobHandle, RunningJob};
use crate::runtime::runner_support::{process_group_alive, wait_for_group_exit};
use crate::runtime::terminal::exit_signal;
use crate::store::Store;
use crate::util::time::now_ts;

pub(crate) fn timed_out_jobs(store: &Store, jobs: &HashMap<i64, RunningJob>) -> Vec<i64> {
    let now = now_ts();
    jobs.keys()
        .copied()
        .filter_map(|job_id| {
            let job = store.get_job(job_id).ok().flatten()?;
            let start = job.start_time?;
            let limit = job.time_limit_secs?;
            (job.state == JobState::Running && now >= start.saturating_add(limit as i64))
                .then_some(job_id)
        })
        .collect()
}

pub(crate) fn warning_signal_to_send(
    store: &Store,
    jobs: &HashMap<i64, RunningJob>,
    job_id: i64,
) -> Result<Option<i32>> {
    let Some(job) = store.get_job(job_id)? else {
        return Ok(None);
    };
    let Some(start) = job.start_time else {
        return Ok(None);
    };
    let Some(limit) = job.time_limit_secs else {
        return Ok(None);
    };
    let Some(warning_signal) = &job.warning_signal else {
        return Ok(None);
    };
    let now = now_ts();
    let deadline = start.saturating_add(limit as i64);
    let warn_at = deadline.saturating_sub(warning_signal.seconds_before_end as i64);
    if now >= warn_at && now < deadline {
        return Ok((!jobs
            .get(&job_id)
            .map(|running| running.warning_signal_sent)
            .unwrap_or(true))
        .then_some(warning_signal.signal));
    }
    Ok(None)
}

pub(crate) fn terminate_running_job(
    config: &AppConfig,
    store: &Store,
    job_id: i64,
    running: RunningJob,
    final_state: JobState,
    final_reason: &str,
) -> Result<()> {
    store.mark_state(job_id, JobState::Completing, Some(final_reason))?;

    let pgid = Pid::from_raw(running.pgid);
    let _ = killpg(pgid, Signal::SIGTERM);
    thread::sleep(Duration::from_secs(config.cancel_grace_secs));

    let (exit_code, term_signal) = match running.handle {
        JobHandle::Child(mut child) => {
            if let Some(status) = child.try_wait()? {
                (status.code(), exit_signal(&status))
            } else {
                let _ = killpg(pgid, Signal::SIGKILL);
                let status = child.wait()?;
                (
                    status.code(),
                    exit_signal(&status).or(Some(Signal::SIGKILL as i32)),
                )
            }
        }
        JobHandle::Adopted => {
            if process_group_alive(running.pgid)? {
                let _ = killpg(pgid, Signal::SIGKILL);
                wait_for_group_exit(running.pgid, config.cancel_grace_secs)?;
            }
            (None, Some(Signal::SIGKILL as i32))
        }
    };

    let (final_state, final_reason) = if cgroup_oomed(running.cgroup_path.as_deref()) {
        (JobState::OutOfMemory, "OutOfMemory")
    } else {
        (final_state, final_reason)
    };
    let job = store.mark_finished(
        job_id,
        final_state,
        exit_code,
        term_signal,
        Some(final_reason),
    )?;
    if job.state.is_terminal() {
        notify_job(store.config(), &job)?;
    }
    cleanup_cgroup(running.cgroup_path.as_deref());
    Ok(())
}
