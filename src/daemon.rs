use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::thread;
use std::time::Duration;

use crate::config::AppConfig;
use crate::error::Result;
use crate::ipc::{Request, Response};
use crate::job::SubmitRequest;
use crate::recovery;
use crate::runner::Runner;
use crate::store::Store;

pub fn run(config: AppConfig) -> Result<()> {
    config.ensure_dirs()?;
    if config.socket_path.exists() {
        fs::remove_file(&config.socket_path)?;
    }

    let listener = UnixListener::bind(&config.socket_path)?;
    listener.set_nonblocking(true)?;

    let store = Store::open(config.clone())?;
    let mut runner = Runner::new();
    recovery::recover(&store, &mut runner)?;

    loop {
        handle_requests(&listener, &config, &store, &mut runner)?;
        runner.poll(&store)?;
        runner.reconcile_adopted(&store)?;
        schedule_pending_jobs(&store, &mut runner)?;
        thread::sleep(Duration::from_millis(config.scheduler_interval_ms));
    }
}

fn handle_requests(
    listener: &UnixListener,
    config: &AppConfig,
    store: &Store,
    runner: &mut Runner,
) -> Result<()> {
    loop {
        match listener.accept() {
            Ok((stream, _)) => handle_stream(stream, config, store, runner)?,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn handle_stream(
    mut stream: UnixStream,
    config: &AppConfig,
    store: &Store,
    runner: &mut Runner,
) -> Result<()> {
    let mut request_line = String::new();
    {
        let mut reader = BufReader::new(&stream);
        reader.read_line(&mut request_line)?;
    }

    let request: Request = serde_json::from_str(&request_line)?;
    let response = match request {
        Request::SubmitBatch(request) => {
            let job_id = store.create_job(request)?;
            schedule_pending_jobs(store, runner)?;
            Response::Submitted { job_id }
        }
        Request::SubmitRun { request, immediate } => submit_run(store, runner, request, immediate)?,
        Request::ListJobs { states } => Response::Jobs {
            jobs: store.list_jobs(states.as_deref())?,
        },
        Request::ListAccountingJobs { states, ids } => Response::Jobs {
            jobs: store.list_accounting_jobs(states.as_deref(), ids.as_deref())?,
        },
        Request::Cancel { job_id } => {
            if runner.cancel(config, store, job_id)? {
                Response::Cancelled { job_id }
            } else {
                Response::Error {
                    message: format!("job {job_id} was not found or already finished"),
                }
            }
        }
        Request::NodeInfo => Response::NodeInfo {
            info: store.node_info()?,
        },
    };

    serde_json::to_writer(&mut stream, &response)?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    Ok(())
}

fn schedule_pending_jobs(store: &Store, runner: &mut Runner) -> Result<()> {
    loop {
        let pending_jobs = store.next_pending_jobs()?;
        let next_job = pending_jobs.into_iter().find(|job| {
            resources_fit(
                store,
                &job.partition,
                job.requested_cpus,
                job.requested_memory_mb,
                job.requested_gpus,
            )
            .unwrap_or(false)
        });

        if let Some(job) = next_job {
            runner.launch(store, &job)?;
        } else {
            break;
        }
    }
    Ok(())
}

fn submit_run(
    store: &Store,
    runner: &mut Runner,
    request: SubmitRequest,
    immediate: bool,
) -> Result<Response> {
    let can_start_now = resources_fit(
        store,
        &request.partition,
        request.requested_cpus,
        request.requested_memory_mb,
        request.requested_gpus,
    )?;
    if immediate && !can_start_now {
        return Ok(Response::Error {
            message: "resources are not currently available for --immediate srun".to_string(),
        });
    }

    let job_id = store.create_job(request)?;
    if can_start_now {
        if let Some(job) = store.get_job(job_id)? {
            runner.launch(store, &job)?;
        }
    }

    Ok(Response::Submitted { job_id })
}

fn resources_fit(
    store: &Store,
    partition: &str,
    requested_cpus: u32,
    requested_memory_mb: u64,
    requested_gpus: u32,
) -> Result<bool> {
    let (available_cpus, available_memory_mb, available_gpus) = store.available_resources()?;
    let enough_base =
        requested_cpus <= available_cpus && requested_memory_mb <= available_memory_mb;
    let enough_gpu = match partition {
        "gpu" => requested_gpus <= available_gpus,
        _ => requested_gpus == 0,
    };
    Ok(enough_base && enough_gpu)
}
