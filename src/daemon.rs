use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::thread;
use std::time::Duration;

use crate::config::AppConfig;
use crate::error::Result;
use crate::ipc::{Request, Response};
use crate::job::{JobRecord, JobState, SubmitRequest};
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
        runner.enforce_timeouts(&config, &store)?;
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
        Request::SubmitAlloc { request, immediate } => submit_alloc(store, request, immediate)?,
        Request::StartStep {
            parent_job_id,
            name,
            command,
            cwd,
            user_name,
        } => {
            let job_id = store.create_step(parent_job_id, name, command, cwd, user_name)?;
            Response::Submitted { job_id }
        }
        Request::AdoptAllocation { job_id, pid, pgid } => {
            store.adopt_allocation(job_id, pid, pgid)?;
            if let Some(job) = store.get_job(job_id)? {
                runner.adopt(config, &job);
            }
            Response::Submitted { job_id }
        }
        Request::FinishAllocation {
            job_id,
            state,
            exit_code,
            term_signal,
            state_reason,
        } => {
            store.mark_finished(
                job_id,
                state,
                exit_code,
                term_signal,
                state_reason.as_deref(),
            )?;
            Response::Submitted { job_id }
        }
        Request::ListJobs {
            states,
            ids,
            user_name,
            partitions,
        } => Response::Jobs {
            jobs: store.list_jobs(
                states.as_deref(),
                ids.as_deref(),
                user_name.as_deref(),
                partitions.as_deref(),
            )?,
        },
        Request::ListAccountingJobs {
            states,
            ids,
            user_name,
            partitions,
            start_time,
            end_time,
        } => Response::Jobs {
            jobs: store.list_accounting_jobs(
                states.as_deref(),
                ids.as_deref(),
                user_name.as_deref(),
                partitions.as_deref(),
                start_time,
                end_time,
            )?,
        },
        Request::ListSteps { parent_job_id } => Response::Jobs {
            jobs: store.list_steps(parent_job_id)?,
        },
        Request::GetJob { job_id } => Response::Job {
            job: store.get_job(job_id)?,
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
        let mut started = false;
        for job in pending_jobs {
            if let Some(reason) = pending_block_reason(store, &job)? {
                store.mark_state(job.id, JobState::Pending, Some(reason))?;
                continue;
            }

            let fits = resources_fit(
                store,
                &job.partition,
                job.requested_cpus,
                job.requested_tasks,
                job.requested_memory_mb,
                job.requested_gpus,
            )?;
            if !fits {
                store.mark_state(job.id, JobState::Pending, Some("Resources"))?;
                continue;
            }

            if job.allocation_only {
                store.mark_allocation_running(job.id)?;
            } else {
                runner.launch(store, &job)?;
            }
            started = true;
            break;
        }
        if !started {
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
        request.requested_tasks,
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

fn submit_alloc(store: &Store, request: SubmitRequest, immediate: bool) -> Result<Response> {
    let can_start_now = resources_fit(
        store,
        &request.partition,
        request.requested_cpus,
        request.requested_tasks,
        request.requested_memory_mb,
        request.requested_gpus,
    )?;
    if immediate && !can_start_now {
        return Ok(Response::Error {
            message: "resources are not currently available for --immediate salloc".to_string(),
        });
    }

    let job_id = store.create_job(request)?;
    if can_start_now {
        store.mark_allocation_running(job_id)?;
    }

    Ok(Response::Submitted { job_id })
}

fn resources_fit(
    store: &Store,
    partition: &str,
    requested_cpus: u32,
    requested_tasks: u32,
    requested_memory_mb: u64,
    requested_gpus: u32,
) -> Result<bool> {
    let (available_cpus, available_memory_mb, available_gpus) = store.available_resources()?;
    let total_requested_cpus = requested_cpus.saturating_mul(requested_tasks);
    let enough_base =
        total_requested_cpus <= available_cpus && requested_memory_mb <= available_memory_mb;
    let enough_gpu = match partition {
        _ if store.config().is_gpu_partition(partition) => requested_gpus <= available_gpus,
        _ => requested_gpus == 0,
    };
    Ok(enough_base && enough_gpu)
}

fn pending_block_reason<'a>(store: &'a Store, job: &'a JobRecord) -> Result<Option<&'static str>> {
    if !dependency_satisfied(store, job)? {
        return Ok(Some("Dependency"));
    }

    if let (Some(array_job_id), Some(limit)) = (job.array_job_id, job.array_task_limit) {
        if limit > 0 && store.running_array_tasks(array_job_id)? >= limit {
            return Ok(Some("JobArrayTaskLimit"));
        }
    }

    Ok(None)
}

fn dependency_satisfied(store: &Store, job: &JobRecord) -> Result<bool> {
    let Some(spec) = job
        .dependency
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(true);
    };

    for clause in spec
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if !dependency_clause_satisfied(store, job, clause)? {
            return Ok(false);
        }
    }

    Ok(true)
}

fn dependency_clause_satisfied(store: &Store, job: &JobRecord, clause: &str) -> Result<bool> {
    if clause.eq_ignore_ascii_case("singleton") {
        return store
            .has_active_job_with_name(&job.user_name, &job.name, job.id)
            .map(|active| !active);
    }

    let Some((kind, value)) = clause.split_once(':') else {
        return Ok(false);
    };
    let dependency_ids = value
        .split(':')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            value.parse::<i64>().map_err(|_| {
                crate::error::SlotdError::from(format!("invalid dependency job id: {value}"))
            })
        })
        .collect::<Result<Vec<_>>>()?;
    if dependency_ids.is_empty() {
        return Ok(false);
    }

    for dependency_id in dependency_ids {
        let Some(target) = store.get_job(dependency_id)? else {
            return Ok(false);
        };
        let satisfied = match kind {
            "after" => target.start_time.is_some() || target.state.is_terminal(),
            "afterany" => target.state.is_terminal(),
            "afterok" => target.state == JobState::Completed,
            "afternotok" => target.state.is_terminal() && target.state != JobState::Completed,
            _ => false,
        };
        if !satisfied {
            return Ok(false);
        }
    }

    Ok(true)
}
