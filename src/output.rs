use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::AppConfig;
use crate::job::{JobRecord, JobState, NodeInfo};

pub fn print_squeue_jobs(config: &AppConfig, jobs: &[JobRecord]) {
    println!("JOBID | PARTITION |       NAME |         USER | ST |        TIME | NODELIST(REASON)");
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

pub fn print_sacct_jobs(jobs: &[JobRecord]) {
    println!("JOBID | PARTITION |       NAME |         USER |      STATE | EXITCODE");
    for job in jobs {
        println!(
            "{:>5} | {:>9} | {:>10} | {:>12} | {:>10} | {}",
            job.id,
            truncate(&job.partition, 9),
            truncate(&job.name, 10),
            truncate(&job.user_name, 12),
            truncate(job.state.as_str(), 10),
            format_exit_code(job),
        );
    }
}

pub fn print_sinfo(config: &AppConfig, info: &NodeInfo) {
    println!(" PARTITION |           HOSTNAMES | STATE |                        GRES_USED");
    for partition in &info.partitions {
        let partition_name = if partition.name == config.default_partition() {
            format!("{}*", partition.name)
        } else {
            partition.name.clone()
        };
        println!(
            "{:>10} | {:>19} | {:>5} | {:>32}",
            truncate(&partition_name, 10),
            truncate(&partition.hostname, 19),
            partition.state,
            truncate(&partition.gres_used, 32),
        );
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
