use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Child;

use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;

use crate::app::config::AppConfig;
use crate::app::error::Result;
use crate::model::job::{JobRecord, JobState};
use crate::runtime::cgroup::{cgroup_oomed, cleanup_cgroup};
use crate::runtime::notify::notify_job;
use crate::runtime::runner_launch::{job_status_path, launch_running_job};
use crate::runtime::runner_lifecycle::{
    terminate_running_job, timed_out_jobs, warning_signal_to_send,
};
use crate::runtime::runner_support::{
    job_cgroup_path, process_group_alive, read_process_rss_kb, recovered_terminal_state,
};
use crate::runtime::terminal::{exit_signal, terminal_state_with_reasons};
use crate::store::Store;

pub(crate) struct RunningJob {
    pub pgid: i32,
    pub pid: i32,
    pub cgroup_path: Option<PathBuf>,
    pub status_path: Option<PathBuf>,
    pub warning_signal_sent: bool,
    pub(crate) handle: JobHandle,
}

pub(crate) enum JobHandle {
    Child(Child),
    Adopted,
}

pub struct Runner {
    jobs: HashMap<i64, RunningJob>,
}

pub(crate) use crate::runtime::runner_support::process_group_alive_for_recovery;

impl Runner {
    pub fn new() -> Self {
        Self {
            jobs: HashMap::new(),
        }
    }

    pub fn launch(&mut self, store: &Store, job: &JobRecord) -> Result<()> {
        self.jobs.insert(job.id, launch_running_job(store, job)?);
        Ok(())
    }

    pub fn adopt(&mut self, config: &AppConfig, job: &JobRecord) {
        let pgid = job.pgid.or(job.pid).unwrap_or_default();
        if pgid <= 0 {
            return;
        }

        self.jobs.insert(
            job.id,
            RunningJob {
                pgid,
                pid: job.pid.unwrap_or_default(),
                cgroup_path: job_cgroup_path(config, job.id),
                status_path: job_status_path(job),
                warning_signal_sent: false,
                handle: JobHandle::Adopted,
            },
        );
    }

    pub fn reconcile_adopted(&mut self, store: &Store) -> Result<()> {
        let mut finished = Vec::new();
        for (&job_id, running) in &self.jobs {
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

        for job_id in finished {
            self.jobs.remove(&job_id);
        }

        Ok(())
    }

    pub fn poll(&mut self, store: &Store) -> Result<()> {
        let mut finished = Vec::new();
        for (&job_id, running) in &mut self.jobs {
            if running.pid > 0
                && let Some(max_rss_kb) = read_process_rss_kb(running.pid)
            {
                store.update_max_rss(job_id, max_rss_kb)?;
            }
            match &mut running.handle {
                JobHandle::Child(child) => {
                    if let Some(status) = child.try_wait()? {
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
                        let job = store.mark_finished(
                            job_id,
                            state,
                            exit_code,
                            term_signal,
                            Some(reason),
                        )?;
                        if job.state.is_terminal() {
                            notify_job(store.config(), &job)?;
                        }
                        cleanup_cgroup(running.cgroup_path.as_deref());
                        finished.push(job_id);
                    }
                }
                JobHandle::Adopted => {}
            }
        }

        for job_id in finished {
            self.jobs.remove(&job_id);
        }

        Ok(())
    }

    pub fn enforce_timeouts(&mut self, config: &AppConfig, store: &Store) -> Result<()> {
        for job_id in timed_out_jobs(store, &self.jobs) {
            self.terminate_job(config, store, job_id, JobState::Timeout, "TimeLimit")?;
        }

        let jobs_to_warn = self
            .jobs
            .iter()
            .filter_map(|(&job_id, running)| (!running.warning_signal_sent).then_some(job_id))
            .collect::<Vec<_>>();
        for job_id in jobs_to_warn {
            if let Some(signal) = warning_signal_to_send(store, &self.jobs, job_id)? {
                self.signal_job(job_id, signal)?;
                if let Some(running) = self.jobs.get_mut(&job_id) {
                    running.warning_signal_sent = true;
                }
            }
        }

        Ok(())
    }

    pub fn cancel(&mut self, config: &AppConfig, store: &Store, job_id: i64) -> Result<bool> {
        if store.cancel_pending_job(job_id)? {
            return Ok(true);
        }

        if !self.jobs.contains_key(&job_id) {
            return Ok(false);
        }

        self.terminate_job(
            config,
            store,
            job_id,
            JobState::Cancelled,
            "CancelledByUser",
        )?;
        Ok(true)
    }

    pub fn signal_job(&self, job_id: i64, signal: i32) -> Result<bool> {
        let Some(running) = self.jobs.get(&job_id) else {
            return Ok(false);
        };
        let signal = Signal::try_from(signal).map_err(|_| {
            crate::app::error::SlotdError::from(format!("unsupported signal: {signal}"))
        })?;
        let _ = killpg(Pid::from_raw(running.pgid), signal);
        Ok(true)
    }

    pub fn forget(&mut self, job_id: i64) {
        self.jobs.remove(&job_id);
    }
}

impl Runner {
    fn terminate_job(
        &mut self,
        config: &AppConfig,
        store: &Store,
        job_id: i64,
        final_state: JobState,
        final_reason: &str,
    ) -> Result<()> {
        let Some(running) = self.jobs.remove(&job_id) else {
            return Ok(());
        };
        terminate_running_job(config, store, job_id, running, final_state, final_reason)
    }
}
