use std::process::Command;

use crate::app::config::AppConfig;
use crate::model::job::JobRecord;

pub(crate) fn apply_slurm_env(command: &mut Command, config: &AppConfig, job: &JobRecord) {
    command.env(
        "SLURM_JOB_ID",
        job.parent_job_id.unwrap_or(job.id).to_string(),
    );
    command.env("SLURM_JOB_NAME", &job.name);
    command.env("SLURM_JOB_PARTITION", &job.partition);
    command.env("SLURM_JOB_NODELIST", &config.hostname);
    command.env("SLURM_SUBMIT_DIR", &job.cwd);
    command.env("SLURM_NTASKS", job.requested_tasks.max(1).to_string());
    command.env("SLURM_CPUS_PER_TASK", job.requested_cpus.to_string());
    if let Some(array_job_id) = job.array_job_id {
        command.env("SLURM_ARRAY_JOB_ID", array_job_id.to_string());
    }
    if let Some(array_task_id) = job.array_task_id {
        command.env("SLURM_ARRAY_TASK_ID", array_task_id.to_string());
    }
    command.env("SLURM_STEP_ID", job.step_id.unwrap_or(0).to_string());
}
