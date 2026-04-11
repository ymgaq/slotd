use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::AppConfig;
use crate::job::{JobRecord, JobState, PartitionInfo};

pub fn print_squeue_jobs(
    config: &AppConfig,
    jobs: &[JobRecord],
    fields: &[SqueueField],
    noheader: bool,
) {
    print_table(
        fields.iter().map(|field| TableColumn {
            header: field.header().to_string(),
            width: field.width(),
            right_align: field.right_align(),
        }),
        jobs.iter().map(|job| {
            fields.iter().map(|field| field.render(config, job)).collect::<Vec<_>>()
        }),
        noheader,
    );
}

pub fn print_sacct_jobs(jobs: &[JobRecord], fields: &[SacctField], noheader: bool) {
    print_table(
        fields.iter().map(|field| TableColumn {
            header: field.header().to_string(),
            width: field.width(),
            right_align: field.right_align(),
        }),
        jobs.iter().map(|job| fields.iter().map(|field| field.render(job)).collect::<Vec<_>>()),
        noheader,
    );
}

pub fn print_sinfo(
    config: &AppConfig,
    partitions: &[PartitionInfo],
    fields: &[SinfoField],
    noheader: bool,
) {
    print_table(
        fields.iter().map(|field| TableColumn {
            header: field.header().to_string(),
            width: field.width(),
            right_align: field.right_align(),
        }),
        partitions.iter().map(|partition| {
            fields
                .iter()
                .map(|field| field.render(config, partition))
                .collect::<Vec<_>>()
        }),
        noheader,
    );
}

#[derive(Debug, Clone, Copy)]
pub enum SqueueField {
    JobId,
    Partition,
    Name,
    User,
    StateCompact,
    Time,
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
            Self::NodeListReason => "NODELIST(REASON)",
        }
    }

    pub fn width(self) -> usize {
        match self {
            Self::JobId => 5,
            Self::Partition => 9,
            Self::Name => 10,
            Self::User => 12,
            Self::StateCompact => 2,
            Self::Time => 11,
            Self::NodeListReason => 16,
        }
    }

    pub fn right_align(self) -> bool {
        matches!(self, Self::JobId | Self::Time)
    }

    pub fn render(self, config: &AppConfig, job: &JobRecord) -> String {
        match self {
            Self::JobId => job.id.to_string(),
            Self::Partition => job.partition.clone(),
            Self::Name => job.name.clone(),
            Self::User => job.user_name.clone(),
            Self::StateCompact => job.state.short_code().to_string(),
            Self::Time => format_elapsed(job),
            Self::NodeListReason => squeue_nodelist_reason(config, job),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SacctField {
    JobId,
    JobName,
    Partition,
    User,
    State,
    ExitCode,
    Elapsed,
}

impl SacctField {
    pub fn header(self) -> &'static str {
        match self {
            Self::JobId => "JobID",
            Self::JobName => "JobName",
            Self::Partition => "Partition",
            Self::User => "User",
            Self::State => "State",
            Self::ExitCode => "ExitCode",
            Self::Elapsed => "Elapsed",
        }
    }

    pub fn render(self, job: &JobRecord) -> String {
        match self {
            Self::JobId => job.id.to_string(),
            Self::JobName => job.name.clone(),
            Self::Partition => job.partition.clone(),
            Self::User => job.user_name.clone(),
            Self::State => job.state.as_str().to_string(),
            Self::ExitCode => format_exit_code(job),
            Self::Elapsed => format_elapsed(job),
        }
    }

    pub fn width(self) -> usize {
        match self {
            Self::JobId => 5,
            Self::JobName => 10,
            Self::Partition => 9,
            Self::User => 12,
            Self::State => 10,
            Self::ExitCode => 8,
            Self::Elapsed => 11,
        }
    }

    pub fn right_align(self) -> bool {
        matches!(self, Self::JobId | Self::ExitCode | Self::Elapsed)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SinfoField {
    Partition,
    Hostnames,
    State,
    GresUsed,
}

impl SinfoField {
    pub fn header(self) -> &'static str {
        match self {
            Self::Partition => "PARTITION",
            Self::Hostnames => "HOSTNAMES",
            Self::State => "STATE",
            Self::GresUsed => "GRES_USED",
        }
    }

    pub fn width(self) -> usize {
        match self {
            Self::Partition => 10,
            Self::Hostnames => 15,
            Self::State => 5,
            Self::GresUsed => 25,
        }
    }

    pub fn right_align(self) -> bool {
        false
    }

    pub fn render(self, config: &AppConfig, partition: &PartitionInfo) -> String {
        match self {
            Self::Partition => {
                if partition.name == config.default_partition() {
                    format!("{}*", partition.name)
                } else {
                    partition.name.clone()
                }
            }
            Self::Hostnames => partition.hostname.clone(),
            Self::State => partition.state.clone(),
            Self::GresUsed => partition.gres_used.clone(),
        }
    }
}

pub fn parse_squeue_fields(value: Option<&str>) -> std::result::Result<Vec<SqueueField>, String> {
    match value {
        None => Ok(vec![
            SqueueField::JobId,
            SqueueField::Partition,
            SqueueField::Name,
            SqueueField::User,
            SqueueField::StateCompact,
            SqueueField::Time,
            SqueueField::NodeListReason,
        ]),
        Some(spec) => spec
            .split(',')
            .map(|field| match field.trim().to_ascii_lowercase().as_str() {
                "jobid" => Ok(SqueueField::JobId),
                "partition" => Ok(SqueueField::Partition),
                "name" | "jobname" => Ok(SqueueField::Name),
                "user" => Ok(SqueueField::User),
                "st" | "state" => Ok(SqueueField::StateCompact),
                "time" | "elapsed" => Ok(SqueueField::Time),
                "nodelist(reason)" | "nodelistreason" | "reason" | "nodelist" => {
                    Ok(SqueueField::NodeListReason)
                }
                other => Err(format!("unsupported squeue field: {other}")),
            })
            .collect(),
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
        Some(spec) => spec
            .split(',')
            .map(|field| match field.trim().to_ascii_lowercase().as_str() {
                "jobid" => Ok(SacctField::JobId),
                "jobname" => Ok(SacctField::JobName),
                "partition" => Ok(SacctField::Partition),
                "user" => Ok(SacctField::User),
                "state" => Ok(SacctField::State),
                "exitcode" => Ok(SacctField::ExitCode),
                "elapsed" => Ok(SacctField::Elapsed),
                other => Err(format!("unsupported sacct field: {other}")),
            })
            .collect(),
    }
}

pub fn parse_sinfo_fields(value: Option<&str>) -> std::result::Result<Vec<SinfoField>, String> {
    match value {
        None => Ok(vec![
            SinfoField::Partition,
            SinfoField::Hostnames,
            SinfoField::State,
            SinfoField::GresUsed,
        ]),
        Some(spec) => spec
            .split(',')
            .map(|field| match field.trim().to_ascii_lowercase().as_str() {
                "partition" => Ok(SinfoField::Partition),
                "hostnames" | "hostname" | "nodelist" => Ok(SinfoField::Hostnames),
                "state" => Ok(SinfoField::State),
                "gres_used" | "gresused" => Ok(SinfoField::GresUsed),
                other => Err(format!("unsupported sinfo field: {other}")),
            })
            .collect(),
    }
}

#[derive(Debug)]
struct TableColumn {
    header: String,
    width: usize,
    right_align: bool,
}

fn print_table<I, R>(columns: I, rows: R, noheader: bool)
where
    I: IntoIterator<Item = TableColumn>,
    R: IntoIterator<Item = Vec<String>>,
{
    let columns = columns.into_iter().collect::<Vec<_>>();
    if !noheader {
        let header = columns
            .iter()
            .map(|column| format_cell(&column.header, column.width, column.right_align))
            .collect::<Vec<_>>()
            .join(" | ");
        println!("{header}");
    }

    for row in rows {
        let line = row
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let column = &columns[index];
                format_cell(value, column.width, column.right_align)
            })
            .collect::<Vec<_>>()
            .join(" | ");
        println!("{line}");
    }
}

fn truncate(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_string();
    }
    let mut output = value
        .chars()
        .take(width.saturating_sub(1))
        .collect::<String>();
    output.push('-');
    output
}

fn format_cell(value: &str, width: usize, right_align: bool) -> String {
    let value = truncate(value, width);
    if right_align {
        format!("{value:>width$}")
    } else {
        format!("{value:<width$}")
    }
}

fn format_elapsed(job: &JobRecord) -> String {
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
        JobState::Pending => "(Resources)".to_string(),
        JobState::Running => config.hostname.clone(),
        JobState::Cancelled => "(Cancelled)".to_string(),
        JobState::Failed => "(Failed)".to_string(),
        JobState::Completed => config.hostname.clone(),
    }
}

fn format_exit_code(job: &JobRecord) -> String {
    let exit = job.exit_code.unwrap_or(0);
    format!("{exit}:0")
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{
        SacctField, SinfoField, SqueueField, format_cell, parse_sacct_fields, parse_sinfo_fields,
        parse_squeue_fields,
    };

    #[test]
    fn default_sacct_fields_match_expected_order() {
        let fields = parse_sacct_fields(None).expect("default fields");
        assert!(matches!(fields.as_slice(), [
            SacctField::JobId,
            SacctField::Partition,
            SacctField::JobName,
            SacctField::User,
            SacctField::State,
            SacctField::ExitCode
        ]));
    }

    #[test]
    fn parses_custom_sacct_field_list() {
        let fields = parse_sacct_fields(Some("JobID,State,Elapsed")).expect("custom fields");
        assert!(matches!(fields.as_slice(), [
            SacctField::JobId,
            SacctField::State,
            SacctField::Elapsed
        ]));
    }

    #[test]
    fn parses_custom_squeue_field_list() {
        let fields = parse_squeue_fields(Some("JobID,Name,State,Reason")).expect("custom fields");
        assert!(matches!(fields.as_slice(), [
            SqueueField::JobId,
            SqueueField::Name,
            SqueueField::StateCompact,
            SqueueField::NodeListReason
        ]));
    }

    #[test]
    fn parses_custom_sinfo_field_list() {
        let fields = parse_sinfo_fields(Some("Partition,Hostnames,State,GresUsed"))
            .expect("custom fields");
        assert!(matches!(fields.as_slice(), [
            SinfoField::Partition,
            SinfoField::Hostnames,
            SinfoField::State,
            SinfoField::GresUsed
        ]));
    }

    #[test]
    fn formats_cells_with_fixed_width() {
        assert_eq!(format_cell("6", 5, true), "    6");
        assert_eq!(format_cell("gpu", 9, false), "gpu      ");
        assert_eq!(format_cell("CANCELLED", 10, false), "CANCELLED ");
    }
}
