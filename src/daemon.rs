use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::thread;
use std::time::Duration;

use crate::config::AppConfig;
use crate::error::Result;
use crate::ipc::{Request, Response};
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
    recovery::recover(&store)?;
    let mut runner = Runner::new();

    loop {
        handle_requests(&listener, &config, &store, &mut runner)?;
        runner.poll(&store)?;
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
            Response::Submitted { job_id }
        }
        Request::ListJobs => Response::Jobs {
            jobs: store.list_jobs()?,
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
    let (available_cpus, available_memory_mb) = store.available_resources()?;
    if let Some(job) = store.next_pending_job(available_cpus, available_memory_mb)? {
        runner.launch(store, &job)?;
    }
    Ok(())
}
