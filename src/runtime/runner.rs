use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Child;

use crate::app::config::AppConfig;
use crate::app::error::Result;
use crate::model::job::JobRecord;
use crate::runtime::runner_control::{cancel_job, enforce_timeouts, signal_running_job};
use crate::runtime::runner_launch::{job_status_path, launch_running_job};
use crate::runtime::runner_monitor::{poll, reconcile_adopted};
use crate::runtime::runner_support::job_cgroup_path;
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
        reconcile_adopted(self, store)
    }

    pub fn poll(&mut self, store: &Store) -> Result<()> {
        poll(self, store)
    }

    pub fn enforce_timeouts(&mut self, config: &AppConfig, store: &Store) -> Result<()> {
        enforce_timeouts(self, config, store)
    }

    pub fn cancel(&mut self, config: &AppConfig, store: &Store, job_id: i64) -> Result<bool> {
        cancel_job(self, config, store, job_id)
    }

    pub fn signal_job(&self, job_id: i64, signal: i32) -> Result<bool> {
        signal_running_job(&self.jobs, job_id, signal)
    }

    pub fn forget(&mut self, job_id: i64) {
        self.jobs.remove(&job_id);
    }

    pub(crate) fn jobs(&self) -> &HashMap<i64, RunningJob> {
        &self.jobs
    }

    pub(crate) fn jobs_mut(&mut self) -> &mut HashMap<i64, RunningJob> {
        &mut self.jobs
    }
}
