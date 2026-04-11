use crate::job::{JobRecord, NodeInfo};

pub fn print_jobs(jobs: &[JobRecord]) {
    println!(
        "{:<8} {:<18} {:<4} {:>4} {:>8} {:<19} COMMAND",
        "JOBID", "NAME", "ST", "CPU", "MEM", "SUBMIT_TIME"
    );
    for job in jobs {
        println!(
            "{:<8} {:<18} {:<4} {:>4} {:>8} {:<19} {}",
            job.id,
            truncate(&job.name, 18),
            job.state.short_code(),
            job.requested_cpus,
            format!("{}M", job.requested_memory_mb),
            format_timestamp(job.submit_time),
            truncate(&job.command, 40),
        );
    }
}

pub fn print_node_info(info: &NodeInfo) {
    println!("NODE      CPUS   CPU_USED   MEM_MB   MEM_USED   RUNNING   PENDING");
    println!(
        "localhost {:>5} {:>10} {:>8} {:>10} {:>8} {:>8}",
        info.total_cpus,
        info.allocated_cpus,
        info.total_memory_mb,
        info.allocated_memory_mb,
        info.running_jobs,
        info.pending_jobs,
    );
}

fn truncate(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_string();
    }
    let mut output = value
        .chars()
        .take(width.saturating_sub(1))
        .collect::<String>();
    output.push('~');
    output
}

fn format_timestamp(timestamp: i64) -> String {
    format!("{timestamp}")
}
