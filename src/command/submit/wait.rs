use std::thread;
use std::time::Duration;

use crate::app::config::AppConfig;
use crate::app::error::{Result, SlotdError};
use crate::model::job::{JobRecord, JobState};
use crate::proto::ipc::{Request, Response, send_request};

pub(super) fn wait_for_submission_completion(config: &AppConfig, job_id: i64) -> Result<()> {
    let job = wait_for_job_completion(config, job_id)?;
    let mut jobs = vec![job.clone()];

    if job.array_job_id == Some(job.id) && job.array_task_count.unwrap_or(1) > 1 {
        loop {
            match send_request(
                config,
                &Request::ListAccountingJobs {
                    states: None,
                    ids: None,
                    user_name: None,
                    partitions: None,
                    start_time: None,
                    end_time: None,
                },
            )? {
                Response::Jobs { jobs: all_jobs } => {
                    jobs = all_jobs
                        .into_iter()
                        .filter(|entry| entry.array_job_id == Some(job.id))
                        .collect();
                    if jobs.len() as u32 >= job.array_task_count.unwrap_or(0)
                        && jobs.iter().all(|entry| entry.state.is_terminal())
                    {
                        break;
                    }
                }
                Response::Error { message } => return Err(SlotdError::from(message)),
                other => {
                    return Err(SlotdError::from(format!(
                        "unexpected response while waiting for array job {job_id}: {other:?}"
                    )));
                }
            }
            thread::sleep(Duration::from_millis(200));
        }
    }

    if let Some(job) = jobs
        .into_iter()
        .find(|entry| entry.state != JobState::Completed)
    {
        return Err(SlotdError::Exit(job.exit_code.unwrap_or(1)));
    }
    Ok(())
}

pub(super) fn wait_for_job_running(config: &AppConfig, job_id: i64) -> Result<JobRecord> {
    loop {
        match send_request(config, &Request::GetJob { job_id })? {
            Response::Job { job } => match *job {
                Some(job) if job.state == JobState::Running => return Ok(job),
                Some(job) if job.state.is_terminal() => {
                    return Err(SlotdError::from(format!(
                        "allocation {job_id} ended before it became runnable: {}",
                        job.state.as_str()
                    )));
                }
                Some(_) => thread::sleep(Duration::from_millis(200)),
                None => return Err(SlotdError::from(format!("allocation {job_id} disappeared"))),
            },
            Response::Error { message } => return Err(SlotdError::from(message)),
            other => {
                return Err(SlotdError::from(format!(
                    "unexpected response while waiting for allocation {job_id}: {other:?}"
                )));
            }
        }
    }
}

fn wait_for_job_completion(config: &AppConfig, job_id: i64) -> Result<JobRecord> {
    loop {
        match send_request(config, &Request::GetJob { job_id })? {
            Response::Job { job } => match *job {
                Some(job) if job.state.is_terminal() => return Ok(job),
                Some(_) => thread::sleep(Duration::from_millis(200)),
                None => return Err(SlotdError::from(format!("job {job_id} disappeared"))),
            },
            Response::Error { message } => return Err(SlotdError::from(message)),
            other => {
                return Err(SlotdError::from(format!(
                    "unexpected response while waiting for job {job_id}: {other:?}"
                )));
            }
        }
    }
}
