use crate::job::{JobRecord, NodeInfo};

pub fn print_jobs(jobs: &[JobRecord]) {
    println!(
        "{:<8} {:<18} {:<6} {:<4} {:>4} {:>8} {:>4} {:<19} COMMAND",
        "JOBID", "NAME", "PART", "ST", "CPU", "MEM", "GPU", "SUBMIT_TIME"
    );
    for job in jobs {
        println!(
            "{:<8} {:<18} {:<6} {:<4} {:>4} {:>8} {:>4} {:<19} {}",
            job.id,
            truncate(&job.name, 18),
            truncate(&job.partition, 6),
            job.state.short_code(),
            job.requested_cpus,
            format!("{}M", job.requested_memory_mb),
            job.requested_gpus,
            format_timestamp(job.submit_time),
            truncate(&job.command, 40),
        );
    }
}

pub fn print_node_info(info: &NodeInfo) {
    println!("PARTITION CPUS   CPU_USED   MEM_MB   MEM_USED   GPUS   GPU_USED   RUNNING   PENDING");
    for partition in &info.partitions {
        println!(
            "{:<9} {:>5} {:>10} {:>8} {:>10} {:>6} {:>10} {:>8} {:>8}",
            partition.name,
            partition.total_cpus,
            partition.allocated_cpus,
            partition.total_memory_mb,
            partition.allocated_memory_mb,
            partition.total_gpus,
            partition.allocated_gpus,
            partition.running_jobs,
            partition.pending_jobs,
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
    output.push('~');
    output
}

fn format_timestamp(timestamp: i64) -> String {
    format!("{timestamp}")
}
