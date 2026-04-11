use std::collections::HashMap;
use std::fs::File;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use nix::sys::signal::{Signal, killpg};
use nix::unistd::{Pid, setsid};

use crate::config::AppConfig;
use crate::error::Result;
use crate::job::{JobRecord, JobState};
use crate::store::Store;

pub struct RunningJob {
    pub pgid: i32,
    child: Child,
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
        let stderr = File::create(&job.stderr_path)?;

        let mut command = Command::new("/bin/bash");
        command.arg(&job.script_path);
        command.current_dir(&job.cwd);
        command.stdin(Stdio::null());
        command.stdout(Stdio::from(stdout));
        command.stderr(Stdio::from(stderr));
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
        store.mark_running(job.id, pid, pgid)?;

        self.jobs.insert(job.id, RunningJob { pgid, child });
        Ok(())
    }

    pub fn poll(&mut self, store: &Store) -> Result<()> {
        let mut finished = Vec::new();
        for (&job_id, running) in &mut self.jobs {
            if let Some(status) = running.child.try_wait()? {
                let exit_code = exit_signal(&status).or(status.code());
                let state = match exit_code {
                    Some(0) => JobState::Completed,
                    Some(_) => JobState::Failed,
                    None => JobState::Failed,
                };
                store.mark_finished(job_id, state, exit_code)?;
                finished.push(job_id);
            }
        }
        for job_id in finished {
            self.jobs.remove(&job_id);
        }
        Ok(())
    }

    pub fn cancel(&mut self, config: &AppConfig, store: &Store, job_id: i64) -> Result<bool> {
        if store.cancel_pending_job(job_id)? {
            return Ok(true);
        }

        let Some(mut running) = self.jobs.remove(&job_id) else {
            return Ok(false);
        };

        let pgid = Pid::from_raw(running.pgid);
        let _ = killpg(pgid, Signal::SIGTERM);
        thread::sleep(Duration::from_secs(config.cancel_grace_secs));

        let exit_code = if let Some(status) = running.child.try_wait()? {
            exit_signal(&status).or(status.code())
        } else {
            let _ = killpg(pgid, Signal::SIGKILL);
            let status = running.child.wait()?;
            exit_signal(&status).or(status.code())
        };

        store.mark_finished(job_id, JobState::Cancelled, exit_code)?;
        Ok(true)
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
