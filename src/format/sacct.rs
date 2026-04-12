use crate::app::config::AppConfig;
use crate::model::display::{
    display_job_id, format_exit_status, format_job_alloc_tres, format_job_req_tres,
};
use crate::model::job::{JobRecord, JobState};
use crate::util::time::format_timestamp;

use super::squeue::format_elapsed;
use super::table::{TableColumn, print_table};

#[derive(Debug, Clone, Copy)]
pub enum SacctField {
    JobId,
    ArrayJobId,
    ArrayTaskId,
    JobName,
    Partition,
    User,
    State,
    Reason,
    ExitCode,
    Elapsed,
    AllocCpus,
    ReqMem,
    ReqTres,
    AllocTres,
    NodeList,
    Submit,
    Start,
    End,
    WorkDir,
    BatchFlag,
    MaxRss,
}

impl SacctField {
    pub fn header(self) -> &'static str {
        match self {
            Self::JobId => "JobID",
            Self::ArrayJobId => "ArrayJobID",
            Self::ArrayTaskId => "ArrayTaskID",
            Self::JobName => "JobName",
            Self::Partition => "Partition",
            Self::User => "User",
            Self::State => "State",
            Self::Reason => "Reason",
            Self::ExitCode => "ExitCode",
            Self::Elapsed => "Elapsed",
            Self::AllocCpus => "AllocCPUS",
            Self::ReqMem => "ReqMem",
            Self::ReqTres => "ReqTRES",
            Self::AllocTres => "AllocTRES",
            Self::NodeList => "NodeList",
            Self::Submit => "Submit",
            Self::Start => "Start",
            Self::End => "End",
            Self::WorkDir => "WorkDir",
            Self::BatchFlag => "BatchFlag",
            Self::MaxRss => "MaxRSS",
        }
    }
    pub fn render(self, config: &AppConfig, job: &JobRecord) -> String {
        match self {
            Self::JobId => display_job_id(job),
            Self::ArrayJobId => job
                .array_job_id
                .map(|value| value.to_string())
                .unwrap_or_default(),
            Self::ArrayTaskId => job
                .array_task_id
                .map(|value| value.to_string())
                .unwrap_or_default(),
            Self::JobName => job.name.clone(),
            Self::Partition => job.partition.clone(),
            Self::User => job.user_name.clone(),
            Self::State => job.state.as_str().to_string(),
            Self::Reason => job.state_reason.clone().unwrap_or_default(),
            Self::ExitCode => format_exit_status(job),
            Self::Elapsed => format_elapsed(job),
            Self::AllocCpus => job
                .requested_cpus
                .saturating_mul(job.requested_tasks)
                .to_string(),
            Self::ReqMem => format!("{}M", job.requested_memory_mb),
            Self::ReqTres => format_job_req_tres(job),
            Self::AllocTres => format_job_alloc_tres(config, job),
            Self::NodeList => {
                if matches!(job.state, JobState::Pending) {
                    String::new()
                } else {
                    config.hostname.clone()
                }
            }
            Self::Submit => format_timestamp(job.submit_time),
            Self::Start => job.start_time.map(format_timestamp).unwrap_or_default(),
            Self::End => job.end_time.map(format_timestamp).unwrap_or_default(),
            Self::WorkDir => job.cwd.clone(),
            Self::BatchFlag => if job.allocation_only { "0" } else { "1" }.to_string(),
            Self::MaxRss => job
                .max_rss_kb
                .map(|value| format!("{value}K"))
                .unwrap_or_default(),
        }
    }
    pub fn width(self) -> usize {
        match self {
            Self::JobId => 12,
            Self::ArrayJobId => 10,
            Self::ArrayTaskId => 11,
            Self::JobName => 10,
            Self::Partition => 9,
            Self::User => 12,
            Self::State => 10,
            Self::Reason => 16,
            Self::ExitCode => 8,
            Self::Elapsed => 11,
            Self::AllocCpus => 9,
            Self::ReqMem => 8,
            Self::ReqTres => 24,
            Self::AllocTres => 24,
            Self::NodeList => 12,
            Self::Submit => 19,
            Self::Start => 19,
            Self::End => 19,
            Self::WorkDir => 24,
            Self::BatchFlag => 9,
            Self::MaxRss => 10,
        }
    }
    pub fn right_align(self) -> bool {
        matches!(
            self,
            Self::JobId
                | Self::ArrayJobId
                | Self::ArrayTaskId
                | Self::ExitCode
                | Self::Elapsed
                | Self::AllocCpus
                | Self::ReqMem
                | Self::BatchFlag
                | Self::MaxRss
        )
    }
}

pub fn print_sacct_jobs(
    config: &AppConfig,
    jobs: &[JobRecord],
    fields: &[SacctField],
    noheader: bool,
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
                .map(|field| field.render(config, job))
                .collect::<Vec<_>>()
        }),
        noheader,
    );
}

pub fn print_sacct_jobs_delimited(
    config: &AppConfig,
    jobs: &[JobRecord],
    fields: &[SacctField],
    noheader: bool,
    delimiter: &str,
) {
    if !noheader {
        println!(
            "{}",
            fields
                .iter()
                .map(|field| field.header().to_string())
                .collect::<Vec<_>>()
                .join(delimiter)
        );
    }
    for job in jobs {
        println!(
            "{}",
            fields
                .iter()
                .map(|field| field.render(config, job))
                .collect::<Vec<_>>()
                .join(delimiter)
        );
    }
}

pub fn parse_sacct_fields(value: Option<&str>) -> std::result::Result<Vec<SacctField>, String> {
    match value {
        None => Ok(vec![
            SacctField::JobId,
            SacctField::Partition,
            SacctField::JobName,
            SacctField::User,
            SacctField::State,
            SacctField::ExitCode,
        ]),
        Some(spec) if spec.contains('%') => parse_percent_sacct_fields(spec),
        Some(spec) => spec
            .split(',')
            .map(|field| match field.trim().to_ascii_lowercase().as_str() {
                "jobid" => Ok(SacctField::JobId),
                "arrayjobid" => Ok(SacctField::ArrayJobId),
                "arraytaskid" => Ok(SacctField::ArrayTaskId),
                "jobname" => Ok(SacctField::JobName),
                "partition" => Ok(SacctField::Partition),
                "user" => Ok(SacctField::User),
                "state" => Ok(SacctField::State),
                "reason" => Ok(SacctField::Reason),
                "exitcode" => Ok(SacctField::ExitCode),
                "elapsed" => Ok(SacctField::Elapsed),
                "alloccpus" => Ok(SacctField::AllocCpus),
                "reqmem" => Ok(SacctField::ReqMem),
                "reqtres" => Ok(SacctField::ReqTres),
                "alloctres" => Ok(SacctField::AllocTres),
                "nodelist" => Ok(SacctField::NodeList),
                "submit" => Ok(SacctField::Submit),
                "start" => Ok(SacctField::Start),
                "end" => Ok(SacctField::End),
                "workdir" => Ok(SacctField::WorkDir),
                "batchflag" => Ok(SacctField::BatchFlag),
                "maxrss" => Ok(SacctField::MaxRss),
                other => Err(format!("unsupported sacct field: {other}")),
            })
            .collect(),
    }
}

fn parse_percent_sacct_fields(spec: &str) -> std::result::Result<Vec<SacctField>, String> {
    super::parse_percent_tokens(spec)?
        .into_iter()
        .map(|code| match code {
            'i' => Ok(SacctField::JobId),
            'F' => Ok(SacctField::ArrayJobId),
            'K' => Ok(SacctField::ArrayTaskId),
            'j' => Ok(SacctField::JobName),
            'P' => Ok(SacctField::Partition),
            'u' => Ok(SacctField::User),
            't' | 'T' => Ok(SacctField::State),
            'R' => Ok(SacctField::Reason),
            'X' => Ok(SacctField::ExitCode),
            'M' => Ok(SacctField::Elapsed),
            'C' => Ok(SacctField::AllocCpus),
            'm' => Ok(SacctField::ReqMem),
            'b' => Ok(SacctField::ReqTres),
            'B' => Ok(SacctField::AllocTres),
            'N' => Ok(SacctField::NodeList),
            'V' => Ok(SacctField::Submit),
            'S' => Ok(SacctField::Start),
            'E' => Ok(SacctField::End),
            'Z' => Ok(SacctField::WorkDir),
            other => Err(format!("unsupported sacct format code: %{other}")),
        })
        .collect()
}
