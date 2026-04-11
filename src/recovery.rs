use crate::error::Result;
use crate::job::JobState;
use crate::notify::notify_job;
use crate::runner::{Runner, process_group_alive_for_recovery};
use crate::store::Store;
use std::path::Path;

pub fn recover(store: &Store, runner: &mut Runner) -> Result<()> {
    for job in store.list_running_jobs()? {
        if let Some(pgid) = job.pgid.or(job.pid) {
            if process_group_alive_for_recovery(pgid)? {
                runner.adopt(store.config(), &job);
            } else {
                let exit_code = if job.script_path.is_empty() {
                    None
                } else {
                    let status_path = Path::new(&job.script_path)
                        .parent()
                        .unwrap_or_else(|| Path::new("."))
                        .join("exit_status");
                    std::fs::read_to_string(&status_path)
                        .ok()
                        .and_then(|value| value.trim().parse::<i32>().ok())
                };
                let (state, reason) = match exit_code {
                    Some(0) => (JobState::Completed, "Completed"),
                    Some(_) => (JobState::Failed, "NonZeroExitCode"),
                    None => (JobState::Failed, "LostAfterRestart"),
                };
                let job = store.mark_finished(job.id, state, exit_code, None, Some(reason))?;
                if job.state.is_terminal() {
                    notify_job(store.config(), &job)?;
                }
            }
        } else {
            let job = store.mark_finished(
                job.id,
                JobState::Failed,
                None,
                None,
                Some("LostAfterRestart"),
            )?;
            if job.state.is_terminal() {
                notify_job(store.config(), &job)?;
            }
        }
    }

    Ok(())
}
