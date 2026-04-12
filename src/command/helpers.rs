use std::collections::HashMap;
use std::path::PathBuf;

use crate::app::config::AppConfig;
use crate::app::error::{Result, SlotdError};
use crate::command::args::SbatchEnvOverrides;
use crate::model::display::{format_exit_status, format_job_alloc_tres, format_job_req_tres};
use crate::model::job::{JobRecord, JobState, PartitionInfo};
use crate::proto::ipc::{Request, Response, send_request};
use crate::submit::sbatch::{BatchDirectives, parse_mem_mb, parse_time_limit_secs};
use crate::util::env::parse_env_flag;
use crate::util::time::{format_timestamp, now_ts};

pub(crate) fn parse_states(values: Vec<String>) -> Result<Vec<JobState>> {
    values
        .into_iter()
        .map(|value| parse_state(&value))
        .collect()
}

pub(crate) fn validate_constraint(config: &AppConfig, value: &str, partition: &str) -> Result<()> {
    if config.matches_constraint(value, partition) {
        Ok(())
    } else {
        Err(SlotdError::from(format!(
            "constraint {value:?} does not match local features for partition {partition}"
        )))
    }
}

pub(crate) fn load_sbatch_env_overrides() -> SbatchEnvOverrides {
    load_sbatch_env_overrides_with(|name| std::env::var(name).ok())
}

pub(crate) fn load_sbatch_env_overrides_with<F>(get: F) -> SbatchEnvOverrides
where
    F: Fn(&str) -> Option<String>,
{
    let directives = BatchDirectives {
        job_name: get("SBATCH_JOB_NAME"),
        partition: get("SBATCH_PARTITION"),
        cpus_per_task: get("SBATCH_CPUS_PER_TASK").and_then(|value| value.parse().ok()),
        ntasks: get("SBATCH_NTASKS").and_then(|value| value.parse().ok()),
        mem_mb: get("SBATCH_MEM").and_then(|value| parse_mem_mb(&value).ok()),
        time_limit_secs: get("SBATCH_TIME").and_then(|value| parse_time_limit_secs(&value).ok()),
        gpus: get("SBATCH_GPUS").and_then(|value| value.parse().ok()),
        constraint: get("SBATCH_CONSTRAINT"),
        begin: get("SBATCH_BEGIN"),
        exclusive: get("SBATCH_EXCLUSIVE")
            .map(|value| parse_env_flag(&value))
            .unwrap_or(false),
        requeue: get("SBATCH_REQUEUE")
            .map(|value| parse_env_flag(&value))
            .unwrap_or(false),
        output_path: get("SBATCH_OUTPUT"),
        error_path: get("SBATCH_ERROR"),
        chdir: get("SBATCH_CHDIR"),
        dependency: get("SBATCH_DEPENDENCY"),
        array_spec: get("SBATCH_ARRAY_INX"),
    };
    let exclusive = directives.exclusive;
    let requeue = directives.requeue;

    SbatchEnvOverrides {
        directives,
        export: get("SBATCH_EXPORT"),
        export_file: get("SBATCH_EXPORT_FILE").map(PathBuf::from),
        open_mode: get("SBATCH_OPEN_MODE"),
        signal: get("SBATCH_SIGNAL"),
        begin: get("SBATCH_BEGIN"),
        exclusive,
        requeue,
    }
}

pub(crate) fn merge_batch_directives(
    directives: &BatchDirectives,
    overrides: &BatchDirectives,
) -> BatchDirectives {
    BatchDirectives {
        job_name: overrides
            .job_name
            .clone()
            .or_else(|| directives.job_name.clone()),
        partition: overrides
            .partition
            .clone()
            .or_else(|| directives.partition.clone()),
        cpus_per_task: overrides.cpus_per_task.or(directives.cpus_per_task),
        ntasks: overrides.ntasks.or(directives.ntasks),
        mem_mb: overrides.mem_mb.or(directives.mem_mb),
        gpus: overrides.gpus.or(directives.gpus),
        constraint: overrides
            .constraint
            .clone()
            .or_else(|| directives.constraint.clone()),
        begin: overrides.begin.clone().or_else(|| directives.begin.clone()),
        exclusive: overrides.exclusive || directives.exclusive,
        requeue: overrides.requeue || directives.requeue,
        time_limit_secs: overrides.time_limit_secs.or(directives.time_limit_secs),
        dependency: overrides
            .dependency
            .clone()
            .or_else(|| directives.dependency.clone()),
        array_spec: overrides
            .array_spec
            .clone()
            .or_else(|| directives.array_spec.clone()),
        output_path: overrides
            .output_path
            .clone()
            .or_else(|| directives.output_path.clone()),
        error_path: overrides
            .error_path
            .clone()
            .or_else(|| directives.error_path.clone()),
        chdir: overrides.chdir.clone().or_else(|| directives.chdir.clone()),
    }
}

pub(crate) fn estimate_start_times(config: &AppConfig, jobs: &[JobRecord]) -> HashMap<i64, String> {
    let mut result = HashMap::new();
    let now = now_ts();
    let running_jobs = jobs
        .iter()
        .filter(|job| job.state == JobState::Running && job.parent_job_id.is_none())
        .collect::<Vec<_>>();
    let used_cpus = running_jobs
        .iter()
        .map(|job| job.requested_cpus.saturating_mul(job.requested_tasks))
        .sum::<u32>();
    let used_memory_mb = running_jobs
        .iter()
        .map(|job| job.requested_memory_mb)
        .sum::<u64>();
    let used_gpus = running_jobs
        .iter()
        .map(|job| job.requested_gpus)
        .sum::<u32>();
    let running_release = running_jobs
        .iter()
        .filter_map(|job| Some(job.start_time?.saturating_add(job.time_limit_secs? as i64)))
        .max();

    for job in jobs {
        let value = match job.state {
            JobState::Running => job.start_time.map(format_timestamp),
            JobState::Pending => {
                let begin_time = job.begin_time.filter(|value| *value > now);
                let fits_now = job.requested_cpus.saturating_mul(job.requested_tasks)
                    <= config.total_cpus.saturating_sub(used_cpus)
                    && job.requested_memory_mb
                        <= config.total_memory_mb.saturating_sub(used_memory_mb)
                    && job.requested_gpus <= config.total_gpus.saturating_sub(used_gpus);
                if fits_now {
                    Some(format_timestamp(begin_time.unwrap_or(now)))
                } else {
                    match (running_release, begin_time) {
                        (Some(release), Some(begin)) => Some(format_timestamp(release.max(begin))),
                        (Some(release), None) => Some(format_timestamp(release)),
                        (None, Some(begin)) => Some(format_timestamp(begin)),
                        (None, None) => None,
                    }
                }
            }
            _ => None,
        };
        result.insert(job.id, value.unwrap_or_else(|| "N/A".to_string()));
    }

    result
}

fn parse_state(value: &str) -> Result<JobState> {
    let normalized = value.trim().to_ascii_uppercase();
    match normalized.as_str() {
        "PD" | "PENDING" => Ok(JobState::Pending),
        "R" | "RUNNING" => Ok(JobState::Running),
        "CG" | "COMPLETING" => Ok(JobState::Completing),
        "CD" | "COMPLETED" => Ok(JobState::Completed),
        "F" | "FAILED" => Ok(JobState::Failed),
        "CA" | "CANCELLED" => Ok(JobState::Cancelled),
        "TO" | "TIMEOUT" => Ok(JobState::Timeout),
        "OOM" | "OUT_OF_MEMORY" => Ok(JobState::OutOfMemory),
        _ => Err(SlotdError::from(format!("unknown state: {value}"))),
    }
}

pub(crate) fn filter_partitions(
    partitions: Vec<PartitionInfo>,
    filters: Option<&[String]>,
) -> Vec<PartitionInfo> {
    partitions
        .into_iter()
        .filter(|partition| {
            filters
                .map(|filters| filters.iter().any(|filter| filter == &partition.name))
                .unwrap_or(true)
        })
        .collect()
}

pub(crate) fn sort_squeue_jobs(mut jobs: Vec<JobRecord>, sort: Option<&str>) -> Vec<JobRecord> {
    let Some(sort) = sort.map(str::trim).filter(|value| !value.is_empty()) else {
        return jobs;
    };
    let descending = sort.starts_with('-');
    let key = sort.trim_start_matches(['+', '-']);
    jobs.sort_by(|a, b| {
        let ordering = match key.to_ascii_lowercase().as_str() {
            "i" | "jobid" => a.id.cmp(&b.id),
            "p" | "partition" => a.partition.cmp(&b.partition),
            "u" | "user" => a.user_name.cmp(&b.user_name),
            "t" | "state" => a.state.as_str().cmp(b.state.as_str()),
            "m" | "time" => a.start_time.cmp(&b.start_time),
            _ => a.id.cmp(&b.id),
        };
        if descending {
            ordering.reverse()
        } else {
            ordering
        }
    });
    jobs
}

pub(crate) fn resolve_job_reference(config: &AppConfig, value: &str) -> Result<i64> {
    if let Some((parent, step)) = value.split_once('.') {
        let parent_job_id = parent
            .parse::<i64>()
            .map_err(|_| SlotdError::from(format!("invalid job id: {value}")))?;
        let step_id = step
            .parse::<u32>()
            .map_err(|_| SlotdError::from(format!("invalid step id: {value}")))?;
        return match send_request(config, &Request::ListSteps { parent_job_id })? {
            Response::Jobs { jobs } => jobs
                .into_iter()
                .find(|job| job.step_id == Some(step_id))
                .map(|job| job.id)
                .ok_or_else(|| SlotdError::from(format!("unknown step reference: {value}"))),
            Response::Error { message } => Err(SlotdError::from(message)),
            other => Err(SlotdError::from(format!(
                "unexpected response while resolving step reference {value}: {other:?}"
            ))),
        };
    }

    value
        .parse::<i64>()
        .map_err(|_| SlotdError::from(format!("invalid job id: {value}")))
}

pub(crate) fn load_job(config: &AppConfig, job_id: i64) -> Result<JobRecord> {
    match send_request(config, &Request::GetJob { job_id })? {
        Response::Job { job } => {
            (*job).ok_or_else(|| SlotdError::from(format!("job {job_id} not found")))
        }
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response while loading job {job_id}: {other:?}"
        ))),
    }
}

pub(crate) fn format_optional_timestamp(value: Option<i64>) -> String {
    value
        .map(format_timestamp)
        .unwrap_or_else(|| "Unknown".to_string())
}

pub(crate) fn print_scontrol_job(config: &AppConfig, job: &JobRecord, steps: &[JobRecord]) {
    let dependency = job.dependency.as_deref().unwrap_or("(null)");
    let reason = job.state_reason.as_deref().unwrap_or("(null)");
    let stdout = if job.stdout_path.is_empty() {
        "(null)"
    } else {
        &job.stdout_path
    };
    let stderr = if job.stderr_path.is_empty() {
        "(null)"
    } else {
        &job.stderr_path
    };
    let node_list = if matches!(job.state, JobState::Pending) {
        "(null)".to_string()
    } else {
        config.hostname.clone()
    };
    let time_limit = job
        .time_limit_secs
        .map(|value| crate::util::time::format_duration_secs(value as i64))
        .unwrap_or_else(|| "UNLIMITED".to_string());
    let array = match (job.array_job_id, job.array_task_id) {
        (Some(array_job_id), Some(array_task_id)) => format!("{array_job_id}_{array_task_id}"),
        _ => "(null)".to_string(),
    };
    let req_tres = format_job_req_tres(job);
    let alloc_tres = format_job_alloc_tres(config, job);
    let req_gres = if job.requested_gpus > 0 {
        format!("gpu:{}", job.requested_gpus)
    } else {
        "(null)".to_string()
    };
    println!(
        "JobId={} JobName={} UserId={}({}) Partition={} State={} Reason={}",
        job.id,
        job.name,
        job.user_name,
        job.user_name,
        job.partition,
        job.state.as_str(),
        reason
    );
    println!(
        "   NumTasks={} CPUs/Task={} ReqMem={}MB ReqGRES={} TimeLimit={} BeginTime={} Dependency={} Exclusive={}",
        job.requested_tasks,
        job.requested_cpus,
        job.requested_memory_mb,
        req_gres,
        time_limit,
        format_optional_timestamp(job.begin_time),
        dependency,
        if job.exclusive { "Yes" } else { "No" }
    );
    println!(
        "   SubmitTime={} StartTime={} EndTime={} ExitCode={} ArrayTask={} BatchFlag={}",
        format_timestamp(job.submit_time),
        format_optional_timestamp(job.start_time),
        format_optional_timestamp(job.end_time),
        format_exit_status(job),
        array,
        if job.allocation_only { 0 } else { 1 }
    );
    println!(
        "   WorkDir={} Command={} StdOut={} StdErr={} NodeList={}",
        job.cwd, job.command, stdout, stderr, node_list
    );
    println!(
        "   ReqTRES={} AllocTRES={} MaxRSS={}",
        req_tres,
        alloc_tres,
        job.max_rss_kb
            .map(|value| format!("{value}K"))
            .unwrap_or_else(|| "(null)".to_string())
    );
    if !steps.is_empty() {
        let summary = steps
            .iter()
            .map(|step| {
                format!(
                    "{}:{}:{}",
                    step.step_id.unwrap_or(0),
                    step.state.as_str(),
                    step.command
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        println!("   Steps={summary}");
    }
}
