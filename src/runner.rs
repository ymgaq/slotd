use std::collections::HashMap;
use std::fs::File;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use nix::errno::Errno;
use nix::sys::signal::{Signal, kill, killpg};
use nix::unistd::{Pid, setsid};

use crate::config::AppConfig;
use crate::error::Result;
use crate::job::{JobRecord, JobState};
use crate::store::Store;

pub struct RunningJob {
    pub pgid: i32,
    handle: JobHandle,
}

enum JobHandle {
    Child(Child),
    Adopted,
}

pub struct Runner {
    jobs: HashMap<i64, RunningJob>,
}

impl Runner {
    pub fn new() -> Self {
        Self {
            jobs: HashMap::new(),
        }
    }

    pub fn launch(&mut self, store: &Store, job: &JobRecord) -> Result<()> {
        let stdout = File::create(&job.stdout_path)?;
        let stderr = if job.stderr_path == job.stdout_path {
            stdout.try_clone()?
        } else {
            File::create(&job.stderr_path)?
        };
        let assigned_gpu_ids = if job.partition == "gpu" {
            store.allocate_gpu_ids(job.requested_gpus)?
        } else {
            Vec::new()
        };

        let mut command = Command::new("/bin/bash");
        command.arg(&job.script_path);
        command.current_dir(&job.cwd);
        command.stdin(Stdio::null());
        command.stdout(Stdio::from(stdout));
        command.stderr(Stdio::from(stderr));
        if !assigned_gpu_ids.is_empty() {
            command.env("CUDA_VISIBLE_DEVICES", join_gpu_ids(&assigned_gpu_ids));
        }
        // Create a dedicated process group so scancel can terminate the whole tree.
        unsafe {
            command.pre_exec(|| {
                setsid().map_err(std::io::Error::other)?;
                Ok(())
            });
        }

        let child = command.spawn()?;
        let pid = child.id() as i32;
        let pgid = pid;
        store.mark_running(job.id, pid, pgid, &assigned_gpu_ids)?;

        self.jobs.insert(
            job.id,
            RunningJob {
                pgid,
                handle: JobHandle::Child(child),
            },
        );
        Ok(())
    }

    pub fn adopt(&mut self, job: &JobRecord) {
        let pgid = job.pgid.or(job.pid).unwrap_or_default();
        if pgid <= 0 {
            return;
        }

        self.jobs.insert(
            job.id,
            RunningJob {
                pgid,
                handle: JobHandle::Adopted,
            },
        );
    }

    pub fn reconcile_adopted(&mut self, store: &Store) -> Result<()> {
        let mut finished = Vec::new();
        for (&job_id, running) in &self.jobs {
            if let JobHandle::Adopted = running.handle {
                if !process_group_alive(running.pgid)? {
                    store.mark_finished(job_id, JobState::Failed, None, None, Some("LostAfterRestart"))?;
                    finished.push(job_id);
                }
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
            match &mut running.handle {
                JobHandle::Child(child) => {
                    if let Some(status) = child.try_wait()? {
                        let exit_code = status.code();
                        let term_signal = exit_signal(&status);
                        let (state, reason) = terminal_state(exit_code, term_signal);
                        store.mark_finished(job_id, state, exit_code, term_signal, Some(reason))?;
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
        let now = now_ts();
        let timed_out = self
            .jobs
            .keys()
            .copied()
            .filter_map(|job_id| {
                let job = store.get_job(job_id).ok().flatten()?;
                let start = job.start_time?;
                let limit = job.time_limit_secs?;
                (job.state == JobState::Running && now >= start.saturating_add(limit as i64))
                    .then_some(job_id)
            })
            .collect::<Vec<_>>();

        for job_id in timed_out {
            self.terminate_job(
                config,
                store,
                job_id,
                JobState::Timeout,
                "TimeLimit",
            )?;
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

        self.terminate_job(config, store, job_id, JobState::Cancelled, "CancelledByUser")?;
        Ok(true)
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
                    (status.code(), exit_signal(&status).or(Some(Signal::SIGKILL as i32)))
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

        store.mark_finished(job_id, final_state, exit_code, term_signal, Some(final_reason))?;
        Ok(())
    }
}

fn terminal_state(exit_code: Option<i32>, term_signal: Option<i32>) -> (JobState, &'static str) {
    match (exit_code, term_signal) {
        (Some(0), None) => (JobState::Completed, "Completed"),
        (Some(_), None) => (JobState::Failed, "NonZeroExitCode"),
        (_, Some(_)) => (JobState::Failed, "Signal"),
        _ => (JobState::Failed, "UnknownFailure"),
    }
}

fn exit_signal(status: &std::process::ExitStatus) -> Option<i32> {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status.signal()
    }
    #[cfg(not(unix))]
    {
        let _ = status;
        None
    }
}

fn process_group_alive(pgid: i32) -> Result<bool> {
    if pgid <= 0 {
        return Ok(false);
    }

    match kill(Pid::from_raw(-pgid), None) {
        Ok(()) => Ok(true),
        Err(Errno::EPERM) => Ok(true),
        Err(Errno::ESRCH) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn wait_for_group_exit(pgid: i32, timeout_secs: u64) -> Result<()> {
    let retries = std::cmp::max(1, timeout_secs * 10);
    for _ in 0..retries {
        if !process_group_alive(pgid)? {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

fn now_ts() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

pub fn process_group_alive_for_recovery(pgid: i32) -> Result<bool> {
    process_group_alive(pgid)
}

fn join_gpu_ids(ids: &[u32]) -> String {
    ids.iter().map(u32::to_string).collect::<Vec<_>>().join(",")
}
