use std::collections::HashMap;

use crate::app::config::AppConfig;
use crate::model::display::display_job_id;
use crate::model::job::{JobRecord, JobState};
use crate::util::time::now_ts;

use super::table::{TableColumn, print_table};

#[derive(Debug, Clone, Copy)]
pub enum SqueueField {
    JobId,
    Partition,
    Name,
    User,
    StateCompact,
    Time,
    TimeLimit,
    NumTasks,
    ReqCpus,
    ReqMem,
    ReqGpus,
    StartTime,
    NodeListReason,
}

impl SqueueField {
    pub fn header(self) -> &'static str {
        match self {
            Self::JobId => "JOBID",
            Self::Partition => "PARTITION",
            Self::Name => "NAME",
            Self::User => "USER",
            Self::StateCompact => "ST",
            Self::Time => "TIME",
            Self::TimeLimit => "TIME_LIMIT",
            Self::NumTasks => "NTASKS",
            Self::ReqCpus => "CPUS",
            Self::ReqMem => "REQ_MEM",
            Self::ReqGpus => "REQ_GPU",
            Self::StartTime => "START_TIME",
            Self::NodeListReason => "NODELIST(REASON)",
        }
    }

    pub fn width(self) -> usize {
        match self {
            Self::JobId => 12,
            Self::Partition => 9,
            Self::Name => 10,
            Self::User => 12,
            Self::StateCompact => 2,
            Self::Time => 11,
            Self::TimeLimit => 11,
            Self::NumTasks => 6,
            Self::ReqCpus => 6,
            Self::ReqMem => 8,
            Self::ReqGpus => 7,
            Self::StartTime => 19,
            Self::NodeListReason => 16,
        }
    }

    pub fn right_align(self) -> bool {
        matches!(
            self,
            Self::JobId
                | Self::Time
                | Self::TimeLimit
                | Self::NumTasks
                | Self::ReqCpus
                | Self::ReqMem
                | Self::ReqGpus
                | Self::StartTime
        )
    }

    pub fn render(self, config: &AppConfig, job: &JobRecord) -> String {
        match self {
            Self::JobId => display_job_id(job),
            Self::Partition => job.partition.clone(),
            Self::Name => job.name.clone(),
            Self::User => job.user_name.clone(),
            Self::StateCompact => job.state.short_code().to_string(),
            Self::Time => format_elapsed(job),
            Self::TimeLimit => job
                .time_limit_secs
                .map(|value| format_duration(value as i64))
                .unwrap_or_else(|| "UNLIMITED".to_string()),
            Self::NumTasks => job.requested_tasks.to_string(),
            Self::ReqCpus => job.requested_cpus.to_string(),
            Self::ReqMem => format!("{}M", job.requested_memory_mb),
            Self::ReqGpus => job.requested_gpus.to_string(),
            Self::StartTime => String::new(),
            Self::NodeListReason => squeue_nodelist_reason(config, job),
        }
    }
}

pub fn print_squeue_jobs(
    config: &AppConfig,
    jobs: &[JobRecord],
    fields: &[SqueueField],
    noheader: bool,
) {
    print_squeue_jobs_with_options(config, jobs, fields, noheader, false);
}

pub fn print_squeue_jobs_with_options(
    config: &AppConfig,
    jobs: &[JobRecord],
    fields: &[SqueueField],
    noheader: bool,
    array_mode: bool,
) {
    print_table(
        fields.iter().map(|field| TableColumn {
            header: field.header().to_string(),
            width: field.width(),
            right_align: field.right_align(),
        }),
        jobs.iter().map(|job| {
            fields
                .iter()
                .map(|field| match field {
                    SqueueField::JobId if array_mode => format_squeue_job_id(job),
                    _ => field.render(config, job),
                })
                .collect::<Vec<_>>()
        }),
        noheader,
    );
}

pub fn print_squeue_jobs_with_start_times(
    config: &AppConfig,
    jobs: &[JobRecord],
    fields: &[SqueueField],
    start_times: &HashMap<i64, String>,
    noheader: bool,
    array_mode: bool,
) {
    print_table(
        fields.iter().map(|field| TableColumn {
            header: field.header().to_string(),
            width: field.width(),
            right_align: field.right_align(),
        }),
        jobs.iter().map(|job| {
            fields
                .iter()
                .map(|field| match field {
                    SqueueField::JobId if array_mode => format_squeue_job_id(job),
                    SqueueField::StartTime => start_times.get(&job.id).cloned().unwrap_or_default(),
                    _ => field.render(config, job),
                })
                .collect::<Vec<_>>()
        }),
        noheader,
    );
}

pub fn parse_squeue_fields(
    value: Option<&str>,
    long: bool,
) -> std::result::Result<Vec<SqueueField>, String> {
    match value {
        None if long => Ok(vec![
            SqueueField::JobId,
            SqueueField::Partition,
            SqueueField::Name,
            SqueueField::User,
            SqueueField::StateCompact,
            SqueueField::Time,
            SqueueField::TimeLimit,
            SqueueField::NumTasks,
            SqueueField::ReqCpus,
            SqueueField::ReqMem,
            SqueueField::ReqGpus,
            SqueueField::NodeListReason,
        ]),
        None => Ok(vec![
            SqueueField::JobId,
            SqueueField::Partition,
            SqueueField::Name,
            SqueueField::User,
            SqueueField::StateCompact,
            SqueueField::Time,
            SqueueField::NodeListReason,
        ]),
        Some(spec) if spec.contains('%') => parse_percent_squeue_fields(spec),
        Some(spec) => spec
            .split(',')
            .map(|field| match field.trim().to_ascii_lowercase().as_str() {
                "jobid" => Ok(SqueueField::JobId),
                "partition" => Ok(SqueueField::Partition),
                "name" | "jobname" => Ok(SqueueField::Name),
                "user" => Ok(SqueueField::User),
                "st" | "state" => Ok(SqueueField::StateCompact),
                "time" | "elapsed" => Ok(SqueueField::Time),
                "timelimit" | "time_limit" => Ok(SqueueField::TimeLimit),
                "ntasks" => Ok(SqueueField::NumTasks),
                "cpus" | "reqcpus" => Ok(SqueueField::ReqCpus),
                "reqmem" => Ok(SqueueField::ReqMem),
                "reqgpu" | "reqgpus" => Ok(SqueueField::ReqGpus),
                "start" | "starttime" => Ok(SqueueField::StartTime),
                "nodelist(reason)" | "nodelistreason" | "reason" | "nodelist" => {
                    Ok(SqueueField::NodeListReason)
                }
                other => Err(format!("unsupported squeue field: {other}")),
            })
            .collect(),
    }
}

fn parse_percent_squeue_fields(spec: &str) -> std::result::Result<Vec<SqueueField>, String> {
    super::parse_percent_tokens(spec)?
        .into_iter()
        .map(|code| match code {
            'i' => Ok(SqueueField::JobId),
            'P' => Ok(SqueueField::Partition),
            'j' => Ok(SqueueField::Name),
            'u' => Ok(SqueueField::User),
            't' | 'T' => Ok(SqueueField::StateCompact),
            'M' => Ok(SqueueField::Time),
            'S' => Ok(SqueueField::StartTime),
            'R' | 'N' => Ok(SqueueField::NodeListReason),
            other => Err(format!("unsupported squeue format code: %{other}")),
        })
        .collect()
}

fn format_squeue_job_id(job: &JobRecord) -> String {
    match (job.array_job_id, job.array_task_id) {
        (Some(array_job_id), Some(array_task_id)) => format!("{array_job_id}_{array_task_id}"),
        _ => job.id.to_string(),
    }
}

pub(crate) fn format_elapsed(job: &JobRecord) -> String {
    let now = now_ts();
    let seconds = match job.state {
        JobState::Pending => 0,
        JobState::Running => job
            .start_time
            .map(|start| now.saturating_sub(start))
            .unwrap_or(0),
        _ => match (job.start_time, job.end_time) {
            (Some(start), Some(end)) => end.saturating_sub(start),
            _ => 0,
        },
    };
    format_duration(seconds)
}

fn format_duration(seconds: i64) -> String {
    let total = seconds.max(0) as u64;
    let days = total / 86_400;
    let rem = total % 86_400;
    let hours = rem / 3_600;
    let minutes = (rem % 3_600) / 60;
    let secs = rem % 60;

    if days > 0 {
        format!("{days}-{hours:02}:{minutes:02}:{secs:02}")
    } else {
        format!("{hours}:{minutes:02}:{secs:02}")
    }
}

fn squeue_nodelist_reason(config: &AppConfig, job: &JobRecord) -> String {
    match job.state {
        JobState::Pending => format_reason(job.state_reason.as_deref().unwrap_or("Resources")),
        JobState::Running => config.hostname.clone(),
        JobState::Completing => format_reason(job.state_reason.as_deref().unwrap_or("Completing")),
        JobState::Completed => config.hostname.clone(),
        JobState::Cancelled => format_reason(job.state_reason.as_deref().unwrap_or("Cancelled")),
        JobState::Failed => format_reason(job.state_reason.as_deref().unwrap_or("Failed")),
        JobState::Timeout => format_reason(job.state_reason.as_deref().unwrap_or("TimeLimit")),
        JobState::OutOfMemory => {
            format_reason(job.state_reason.as_deref().unwrap_or("OutOfMemory"))
        }
    }
}

fn format_reason(reason: &str) -> String {
    format!("({reason})")
}
