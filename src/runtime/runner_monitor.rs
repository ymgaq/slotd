use std::collections::HashMap;

use crate::app::error::Result;
use crate::runtime::cgroup::{cgroup_oomed, cleanup_cgroup};
use crate::runtime::notify::notify_job;
use crate::runtime::runner::{JobHandle, Runner, RunningJob};
use crate::runtime::runner_support::{
    process_group_alive, read_process_rss_kb, recovered_terminal_state,
};
use crate::runtime::terminal::{exit_signal, terminal_state_with_reasons};
use crate::store::Store;

pub(crate) fn reconcile_adopted(runner: &mut Runner, store: &Store) -> Result<()> {
    let mut finished = Vec::new();
    for (&job_id, running) in runner.jobs() {
        if let JobHandle::Adopted = running.handle
            && !process_group_alive(running.pgid)?
        {
            let (state, exit_code, reason) =
                recovered_terminal_state(&running.status_path, running.cgroup_path.as_deref());
            let job = store.mark_finished(job_id, state, exit_code, None, Some(reason))?;
            if job.state.is_terminal() {
                notify_job(store.config(), &job)?;
            }
            cleanup_cgroup(running.cgroup_path.as_deref());
            finished.push(job_id);
        }
    }

    forget_finished_jobs(runner.jobs_mut(), finished);
    Ok(())
}

pub(crate) fn poll(runner: &mut Runner, store: &Store) -> Result<()> {
    let mut finished = Vec::new();
    for (&job_id, running) in runner.jobs_mut() {
        update_max_rss(store, job_id, running)?;
        if let Some((state, exit_code, term_signal, reason)) = poll_child_completion(running)? {
            let job = store.mark_finished(job_id, state, exit_code, term_signal, Some(reason))?;
            if job.state.is_terminal() {
                notify_job(store.config(), &job)?;
            }
            cleanup_cgroup(running.cgroup_path.as_deref());
            finished.push(job_id);
        }
    }

    forget_finished_jobs(runner.jobs_mut(), finished);
    Ok(())
}

fn update_max_rss(store: &Store, job_id: i64, running: &RunningJob) -> Result<()> {
    if running.pid > 0
        && let Some(max_rss_kb) = read_process_rss_kb(running.pid)
    {
        store.update_max_rss(job_id, max_rss_kb)?;
    }
    Ok(())
}

fn poll_child_completion(
    running: &mut RunningJob,
) -> Result<
    Option<(
        crate::model::job::JobState,
        Option<i32>,
        Option<i32>,
        &'static str,
    )>,
> {
    let JobHandle::Child(child) = &mut running.handle else {
        return Ok(None);
    };
    let Some(status) = child.try_wait()? else {
        return Ok(None);
    };
    let exit_code = status.code();
    let term_signal = exit_signal(&status);
    let (state, reason) = terminal_state_with_reasons(
        exit_code,
        term_signal,
        cgroup_oomed(running.cgroup_path.as_deref()),
        "Completed",
        "Signal",
        "NonZeroExitCode",
        "UnknownFailure",
    );
    Ok(Some((state, exit_code, term_signal, reason)))
}

fn forget_finished_jobs(jobs: &mut HashMap<i64, RunningJob>, finished: Vec<i64>) {
    for job_id in finished {
        jobs.remove(&job_id);
    }
}
