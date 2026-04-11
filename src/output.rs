use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::AppConfig;
use crate::job::{JobRecord, JobState, NodeInfo};

pub fn print_squeue_jobs(config: &AppConfig, jobs: &[JobRecord], noheader: bool) {
    if !noheader {
        println!("JOBID | PARTITION |       NAME |         USER | ST |        TIME | NODELIST(REASON)");
    }
    for job in jobs {
        println!(
            "{:>5} | {:>9} | {:>10} | {:>12} | {:>2} | {:>11} | {}",
            job.id,
            truncate(&job.partition, 9),
            truncate(&job.name, 10),
            truncate(&job.user_name, 12),
            job.state.short_code(),
            format_elapsed(job),
            squeue_nodelist_reason(config, job),
        );
    }
}

pub fn print_sacct_jobs(jobs: &[JobRecord], fields: &[SacctField], noheader: bool) {
    if !noheader {
        let header = fields
            .iter()
            .map(|field| format_sacct_cell(field.header(), field.width(), field.right_align()))
            .collect::<Vec<_>>()
            .join(" | ");
        println!("{header}");
    }

    for job in jobs {
        let row = fields
            .iter()
            .map(|field| format_sacct_cell(&field.render(job), field.width(), field.right_align()))
            .collect::<Vec<_>>()
            .join(" | ");
        println!("{row}");
    }
}

pub fn print_sinfo(config: &AppConfig, info: &NodeInfo) {
    println!(" PARTITION |       HOSTNAMES | STATE |                 GRES_USED");
    for partition in &info.partitions {
        let partition_name = if partition.name == config.default_partition() {
            format!("{}*", partition.name)
        } else {
            partition.name.clone()
        };
        println!(
            "{:>10} | {:>15} | {:>5} | {:>25}",
            truncate(&partition_name, 10),
            truncate(&partition.hostname, 15),
            partition.state,
            truncate(&partition.gres_used, 25),
        );
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

fn format_sacct_cell(value: &str, width: usize, right_align: bool) -> String {
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
    use super::{SacctField, format_sacct_cell, parse_sacct_fields};

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
    fn formats_sacct_cells_with_fixed_width() {
        assert_eq!(format_sacct_cell("6", 5, true), "    6");
        assert_eq!(format_sacct_cell("gpu", 9, false), "gpu      ");
        assert_eq!(format_sacct_cell("CANCELLED", 10, false), "CANCELLED ");
    }
}
