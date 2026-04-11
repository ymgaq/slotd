use crate::error::Result;
use crate::job::JobState;
use crate::runner::{Runner, process_group_alive_for_recovery};
use crate::store::Store;

pub fn recover(store: &Store, runner: &mut Runner) -> Result<()> {
    for job in store.list_running_jobs()? {
        if let Some(pgid) = job.pgid.or(job.pid) {
            if process_group_alive_for_recovery(pgid)? {
                runner.adopt(&job);
            } else {
                store.mark_finished(job.id, JobState::Failed, None)?;
            }
        } else {
            store.mark_finished(job.id, JobState::Failed, None)?;
        }
    }

    Ok(())
}
