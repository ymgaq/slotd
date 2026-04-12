use crate::config::AppConfig;
use crate::job::{JobRecord, JobState};

pub(crate) fn display_job_id(job: &JobRecord) -> String {
    if let (Some(parent_job_id), Some(step_id)) = (job.parent_job_id, job.step_id) {
        return format!("{parent_job_id}.{step_id}");
    }
    match (job.array_job_id, job.array_task_id) {
        (Some(array_job_id), Some(array_task_id)) => format!("{array_job_id}_{array_task_id}"),
        _ => job.id.to_string(),
    }
}

pub(crate) fn format_exit_status(job: &JobRecord) -> String {
    format!(
        "{}:{}",
        job.exit_code.unwrap_or(0),
        job.term_signal.unwrap_or(0)
    )
}

pub(crate) fn format_job_req_tres(job: &JobRecord) -> String {
    let mut values = vec![
        format!(
            "cpu={}",
            job.requested_cpus.saturating_mul(job.requested_tasks)
        ),
        format!("mem={}M", job.requested_memory_mb),
    ];
    if job.requested_gpus > 0 {
        values.push(format!("gres/gpu={}", job.requested_gpus));
    }
    values.join(",")
}

pub(crate) fn format_job_alloc_tres(config: &AppConfig, job: &JobRecord) -> String {
    let mut values = vec![
        format!(
            "cpu={}",
            job.requested_cpus.saturating_mul(job.requested_tasks)
        ),
        format!("mem={}M", job.requested_memory_mb),
    ];
    if job.state != JobState::Pending {
        values.push("node=1".to_string());
    }
    if config.is_gpu_partition(&job.partition) && job.requested_gpus > 0 {
        values.push(format!("gres/gpu={}", job.requested_gpus));
    }
    values.join(",")
}
