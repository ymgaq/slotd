use std::process::Command;

use crate::config::AppConfig;
use crate::error::Result;
use crate::job::JobRecord;

pub fn notify_job(config: &AppConfig, job: &JobRecord) -> Result<()> {
    if job.parent_job_id.is_some() || !job.state.is_terminal() {
        return Ok(());
    }
    let Some(command) = config.notify_command.as_deref() else {
        return Ok(());
    };

    let mut child = Command::new("/bin/sh");
    child.arg("-lc").arg(command);
    child.env("SLOTD_JOB_ID", job.id.to_string());
    child.env("SLOTD_JOB_NAME", &job.name);
    child.env("SLOTD_JOB_STATE", job.state.as_str());
    child.env("SLOTD_JOB_PARTITION", &job.partition);
    child.env(
        "SLOTD_JOB_REASON",
        job.state_reason.as_deref().unwrap_or_default(),
    );
    let _ = child.spawn()?;
    Ok(())
}
