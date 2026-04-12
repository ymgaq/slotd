use std::fs;
use std::path::Path;

use crate::app::error::{Result, SlotdError};
use crate::model::job::{JobRecord, JobState};

pub(crate) fn default_name(script_name: &str) -> String {
    Path::new(script_name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("batch-job")
        .to_string()
}

pub(crate) fn parse_export_env_json(value: &str) -> Result<Vec<(String, String)>> {
    if value.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(value).map_err(Into::into)
}

pub(crate) fn order_pending_jobs(mut jobs: Vec<JobRecord>) -> Vec<JobRecord> {
    jobs.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| a.submit_time.cmp(&b.submit_time))
            .then_with(|| a.id.cmp(&b.id))
    });

    let mut grouped = std::collections::BTreeMap::<i64, Vec<JobRecord>>::new();
    for job in jobs {
        let group_key = job.array_job_id.unwrap_or(job.id);
        grouped.entry(group_key).or_default().push(job);
    }

    let group_order = grouped
        .iter()
        .map(|(group_key, group_jobs)| {
            let top = &group_jobs[0];
            (*group_key, top.priority, top.submit_time, top.id)
        })
        .collect::<Vec<_>>();

    let mut group_order = group_order;
    group_order.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| a.2.cmp(&b.2))
            .then_with(|| a.3.cmp(&b.3))
    });

    let mut ordered = Vec::new();
    loop {
        let mut progressed = false;
        for (group_key, _, _, _) in &group_order {
            if let Some(group_jobs) = grouped.get_mut(group_key)
                && !group_jobs.is_empty()
            {
                ordered.push(group_jobs.remove(0));
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }

    ordered
}

pub(crate) fn script_command(script_name: &str) -> String {
    format!("bash {script_name}")
}

pub(crate) fn path_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

pub(crate) fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

pub(crate) fn join_gpu_ids(ids: &[u32]) -> String {
    ids.iter().map(u32::to_string).collect::<Vec<_>>().join(",")
}

pub(crate) fn parse_gpu_ids(value: &str) -> Result<Vec<u32>> {
    if value.trim().is_empty() {
        return Ok(Vec::new());
    }

    value
        .split(',')
        .map(|part| {
            part.trim()
                .parse::<u32>()
                .map_err(|_| SlotdError::from(format!("invalid GPU id list: {value}")))
        })
        .collect()
}

pub(crate) fn filter_jobs(
    jobs: Vec<JobRecord>,
    states: Option<&[JobState]>,
    ids: Option<&[i64]>,
    user_name: Option<&str>,
    partitions: Option<&[String]>,
    start_time: Option<i64>,
    end_time: Option<i64>,
) -> Vec<JobRecord> {
    jobs.into_iter()
        .filter(|job| {
            let state_ok = states
                .map(|states| states.contains(&job.state))
                .unwrap_or(true);
            let id_ok = ids.map(|ids| ids.contains(&job.id)).unwrap_or(true);
            let user_ok = user_name.map(|name| job.user_name == name).unwrap_or(true);
            let partition_ok = partitions
                .map(|partitions| {
                    partitions
                        .iter()
                        .any(|partition| partition == &job.partition)
                })
                .unwrap_or(true);
            let start_ok = start_time
                .map(|start| {
                    job.submit_time >= start || job.start_time.unwrap_or(job.submit_time) >= start
                })
                .unwrap_or(true);
            let end_ok = end_time
                .map(|end| {
                    let effective_end = job.end_time.or(job.start_time).unwrap_or(job.submit_time);
                    effective_end <= end
                })
                .unwrap_or(true);
            state_ok && id_ok && user_ok && partition_ok && start_ok && end_ok
        })
        .collect()
}

pub(crate) fn partition_state(
    is_gpu_partition: bool,
    allocated_cpus: u32,
    allocated_gpus: u32,
    total_cpus: u32,
    total_gpus: u32,
) -> String {
    if is_gpu_partition {
        if allocated_gpus == 0 {
            "idle".to_string()
        } else if allocated_gpus >= total_gpus {
            "alloc".to_string()
        } else {
            "mix".to_string()
        }
    } else if allocated_cpus == 0 {
        "idle".to_string()
    } else if allocated_cpus >= total_cpus {
        "alloc".to_string()
    } else {
        "mix".to_string()
    }
}

pub(crate) fn partition_gres_used(
    is_gpu_partition: bool,
    gpu_model: &str,
    total_gpus: u32,
    ids: &[u32],
) -> String {
    if !is_gpu_partition {
        return "N/A".to_string();
    }

    let idx = if ids.is_empty() {
        "IDX:N/A".to_string()
    } else {
        format!(
            "IDX:{}",
            ids.iter().map(u32::to_string).collect::<Vec<_>>().join(",")
        )
    };

    format!("gpu:{gpu_model}:{total_gpus}({idx})")
}
