use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::{FromRawFd, IntoRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::cgroup::{cgroup_oomed, cleanup_cgroup, setup_job_cgroup};
use crate::config::AppConfig;
use crate::cpu::{apply_cpu_affinity, resolve_cpu_bind_ids};
use crate::error::{Result, SlotdError};
use crate::ipc::{Request, Response, send_request};
use crate::job::{JobRecord, JobState};
use crate::launch::{LaunchCommand, build_multitask_launcher, shell_join};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ForegroundIoOptions<'a> {
    pub(crate) stdout_path: Option<&'a Path>,
    pub(crate) stderr_path: Option<&'a Path>,
    pub(crate) label_output: bool,
    pub(crate) unbuffered: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ForegroundExecutionOptions<'a> {
    pub(crate) record_step: bool,
    pub(crate) cpu_bind: Option<&'a str>,
    pub(crate) io: ForegroundIoOptions<'a>,
}

pub(crate) fn run_foreground_allocation(
    config: &AppConfig,
    job: &JobRecord,
    command: &[String],
) -> Result<()> {
    run_foreground_allocation_with_mode(config, job, command, ForegroundExecutionOptions::default())
}

pub(crate) fn run_foreground_allocation_with_mode(
    config: &AppConfig,
    job: &JobRecord,
    command: &[String],
    options: ForegroundExecutionOptions<'_>,
) -> Result<()> {
    let step_record = if options.record_step {
        Some(start_step_record(config, job, command)?)
    } else {
        None
    };
    let launcher_script = build_multitask_launcher(
        LaunchCommand::Command(command),
        job.requested_tasks,
        options.io.label_output,
    );
    let mut command_builder = Command::new("/bin/bash");
    command_builder.arg("-lc").arg(launcher_script);
    command_builder.current_dir(&job.cwd);
    command_builder.stdin(Stdio::inherit());
    let io_state = configure_foreground_stdio(
        &mut command_builder,
        &job.cwd,
        options.io.stdout_path,
        options.io.stderr_path,
        false,
        options.io.unbuffered,
    )?;
    apply_slurm_env(
        &mut command_builder,
        config,
        step_record.as_ref().unwrap_or(job),
    );
    let cpu_ids = resolve_cpu_bind_ids(
        options
            .cpu_bind
            .or(step_record.as_ref().and_then(|step| step.cpu_bind.as_deref())),
        config.total_cpus,
        job.requested_cpus
            .saturating_mul(job.requested_tasks)
            .max(1),
    )?;
    unsafe {
        command_builder.pre_exec(|| {
            nix::unistd::setsid().map_err(std::io::Error::other)?;
            Ok(())
        });
    }
    if let Some(cpu_ids) = cpu_ids {
        unsafe {
            command_builder.pre_exec(move || {
                apply_cpu_affinity(&cpu_ids).map_err(std::io::Error::other)?;
                Ok(())
            });
        }
    }
    let mut child = command_builder.spawn()?;
    drop(command_builder);
    let pid = child.id() as i32;
    let pgid = pid;
    let local_cgroup = match setup_local_cgroup(
        config,
        step_record.as_ref().map(|step| step.id).unwrap_or(job.id),
        step_record.as_ref().unwrap_or(job),
        pid,
    ) {
        Ok(path) => path,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    match send_request(
        config,
        &Request::AdoptAllocation {
            job_id: job.id,
            pid,
            pgid,
        },
    )? {
        Response::Submitted { .. } => {}
        Response::Error { message } => return Err(SlotdError::from(message)),
        other => {
            return Err(SlotdError::from(format!(
                "unexpected response while adopting allocation {}: {other:?}",
                job.id
            )));
        }
    }

    let step_job_id = if let Some(step) = &step_record {
        match send_request(
            config,
            &Request::AdoptAllocation {
                job_id: step.id,
                pid,
                pgid,
            },
        )? {
            Response::Submitted { .. } => Some(step.id),
            Response::Error { message } => return Err(SlotdError::from(message)),
            other => {
                return Err(SlotdError::from(format!(
                    "unexpected response while adopting step {}: {other:?}",
                    step.id
                )));
            }
        }
    } else {
        None
    };

    let status = child.wait()?;
    io_state.finish()?;
    let exit_code = status.code();
    let term_signal = exit_signal(&status);
    let (state, reason) = allocation_terminal_state(
        exit_code,
        term_signal,
        cgroup_oomed(local_cgroup.as_deref()),
    );
    match send_request(
        config,
        &Request::FinishAllocation {
            job_id: job.id,
            state,
            exit_code,
            term_signal,
            state_reason: Some(reason.to_string()),
        },
    )? {
        Response::Submitted { .. } => {}
        Response::Error { message } => return Err(SlotdError::from(message)),
        other => {
            return Err(SlotdError::from(format!(
                "unexpected response while finishing allocation {}: {other:?}",
                job.id
            )));
        }
    }

    if let Some(step_job_id) = step_job_id {
        match send_request(
            config,
            &Request::FinishAllocation {
                job_id: step_job_id,
                state,
                exit_code,
                term_signal,
                state_reason: Some(reason.to_string()),
            },
        )? {
            Response::Submitted { .. } => {}
            Response::Error { message } => return Err(SlotdError::from(message)),
            other => {
                return Err(SlotdError::from(format!(
                    "unexpected response while finishing step {}: {other:?}",
                    step_job_id
                )));
            }
        }
    }
    cleanup_cgroup(local_cgroup.as_deref());

    match state {
        JobState::Completed => Ok(()),
        _ => Err(SlotdError::Exit(exit_code.unwrap_or(1))),
    }
}

pub(crate) fn run_foreground_step(
    config: &AppConfig,
    job: &JobRecord,
    command: &[String],
    options: ForegroundExecutionOptions<'_>,
) -> Result<()> {
    let step = start_step_record(config, job, command)?;
    let launcher_script = build_multitask_launcher(
        LaunchCommand::Command(command),
        step.requested_tasks.max(1),
        options.io.label_output,
    );
    let mut command_builder = Command::new("/bin/bash");
    command_builder.arg("-lc").arg(launcher_script);
    command_builder.current_dir(&job.cwd);
    command_builder.stdin(Stdio::inherit());
    let io_state = configure_foreground_stdio(
        &mut command_builder,
        &job.cwd,
        options.io.stdout_path,
        options.io.stderr_path,
        false,
        options.io.unbuffered,
    )?;
    apply_slurm_env(&mut command_builder, config, &step);
    let cpu_ids = resolve_cpu_bind_ids(
        options.cpu_bind.or(step.cpu_bind.as_deref()),
        config.total_cpus,
        job.requested_cpus
            .saturating_mul(job.requested_tasks)
            .max(1),
    )?;
    unsafe {
        command_builder.pre_exec(|| {
            nix::unistd::setsid().map_err(std::io::Error::other)?;
            Ok(())
        });
    }
    if let Some(cpu_ids) = cpu_ids {
        unsafe {
            command_builder.pre_exec(move || {
                apply_cpu_affinity(&cpu_ids).map_err(std::io::Error::other)?;
                Ok(())
            });
        }
    }
    let mut child = command_builder.spawn()?;
    drop(command_builder);
    let pid = child.id() as i32;
    let pgid = pid;
    let local_cgroup = match setup_local_cgroup(config, step.id, &step, pid) {
        Ok(path) => path,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    match send_request(
        config,
        &Request::AdoptAllocation {
            job_id: step.id,
            pid,
            pgid,
        },
    )? {
        Response::Submitted { .. } => {}
        Response::Error { message } => return Err(SlotdError::from(message)),
        other => {
            return Err(SlotdError::from(format!(
                "unexpected response while adopting step {}: {other:?}",
                step.id
            )));
        }
    }
    let status = child.wait()?;
    io_state.finish()?;
    let exit_code = status.code();
    let term_signal = exit_signal(&status);
    let (state, reason) = allocation_terminal_state(
        exit_code,
        term_signal,
        cgroup_oomed(local_cgroup.as_deref()),
    );
    match send_request(
        config,
        &Request::FinishAllocation {
            job_id: step.id,
            state,
            exit_code,
            term_signal,
            state_reason: Some(reason.to_string()),
        },
    )? {
        Response::Submitted { .. } => {}
        Response::Error { message } => return Err(SlotdError::from(message)),
        other => {
            return Err(SlotdError::from(format!(
                "unexpected response while finishing step {}: {other:?}",
                step.id
            )));
        }
    }
    cleanup_cgroup(local_cgroup.as_deref());
    match status.code() {
        Some(0) => Ok(()),
        Some(code) => Err(SlotdError::Exit(code)),
        None => Err(SlotdError::Exit(1)),
    }
}

fn start_step_record(config: &AppConfig, parent: &JobRecord, command: &[String]) -> Result<JobRecord> {
    let step_job_id = match send_request(
        config,
        &Request::StartStep {
            parent_job_id: parent.id,
            name: command_basename(&command[0]),
            command: shell_join(command),
            cwd: parent.cwd.clone(),
            user_name: current_user_name(),
        },
    )? {
        Response::Submitted { job_id } => job_id,
        Response::Error { message } => return Err(SlotdError::from(message)),
        other => {
            return Err(SlotdError::from(format!(
                "unexpected response while starting step for job {}: {other:?}",
                parent.id
            )));
        }
    };

    match send_request(config, &Request::GetJob { job_id: step_job_id })? {
        Response::Job { job } => {
            (*job).ok_or_else(|| SlotdError::from(format!("step {step_job_id} disappeared")))
        }
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response while loading step {}: {other:?}",
            step_job_id
        ))),
    }
}

fn apply_slurm_env(command: &mut Command, config: &AppConfig, job: &JobRecord) {
    command.env(
        "SLURM_JOB_ID",
        job.parent_job_id.unwrap_or(job.id).to_string(),
    );
    command.env("SLURM_JOB_NAME", &job.name);
    command.env("SLURM_JOB_PARTITION", &job.partition);
    command.env("SLURM_JOB_NODELIST", &config.hostname);
    command.env("SLURM_SUBMIT_DIR", &job.cwd);
    command.env("SLURM_NTASKS", job.requested_tasks.max(1).to_string());
    command.env("SLURM_CPUS_PER_TASK", job.requested_cpus.to_string());
    if let Some(array_job_id) = job.array_job_id {
        command.env("SLURM_ARRAY_JOB_ID", array_job_id.to_string());
    }
    if let Some(array_task_id) = job.array_task_id {
        command.env("SLURM_ARRAY_TASK_ID", array_task_id.to_string());
    }
    command.env("SLURM_STEP_ID", job.step_id.unwrap_or(0).to_string());
}

fn apply_foreground_stdio(
    command: &mut Command,
    cwd: &str,
    stdout_path: Option<&Path>,
    stderr_path: Option<&Path>,
) -> Result<()> {
    let stdout = open_foreground_stdio(stdout_path, cwd, "/dev/stdout")?;
    let stderr = if same_path(stdout_path, stderr_path) {
        stdout.try_clone()?
    } else {
        open_foreground_stdio(stderr_path, cwd, "/dev/stderr")?
    };
    command.stdout(Stdio::from(stdout));
    command.stderr(Stdio::from(stderr));
    Ok(())
}

fn open_foreground_stdio(path: Option<&Path>, cwd: &str, fallback: &str) -> Result<std::fs::File> {
    let Some(path) = path else {
        return open_stdio_handle(fallback);
    };
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        Path::new(cwd).join(path)
    };
    let mut options = OpenOptions::new();
    options.create(true).write(true).truncate(true);
    Ok(options.open(resolved)?)
}

fn same_path(stdout_path: Option<&Path>, stderr_path: Option<&Path>) -> bool {
    match (stdout_path, stderr_path) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

fn open_stdio_handle(path: &str) -> Result<std::fs::File> {
    Ok(OpenOptions::new().write(true).open(path)?)
}

enum ForegroundIoState {
    Direct,
    Streamed(Vec<std::thread::JoinHandle<Result<()>>>),
}

impl ForegroundIoState {
    fn finish(self) -> Result<()> {
        match self {
            Self::Direct => Ok(()),
            Self::Streamed(handles) => {
                for handle in handles {
                    handle
                        .join()
                        .map_err(|_| SlotdError::from("foreground output thread panicked"))??;
                }
                Ok(())
            }
        }
    }
}

fn configure_foreground_stdio(
    command: &mut Command,
    cwd: &str,
    stdout_path: Option<&Path>,
    stderr_path: Option<&Path>,
    label_output: bool,
    unbuffered: bool,
) -> Result<ForegroundIoState> {
    if !label_output && !unbuffered {
        apply_foreground_stdio(command, cwd, stdout_path, stderr_path)?;
        return Ok(ForegroundIoState::Direct);
    }

    let stdout_target = open_foreground_stdio(stdout_path, cwd, "/dev/stdout")?;
    let stderr_target = if same_path(stdout_path, stderr_path) {
        stdout_target.try_clone()?
    } else {
        open_foreground_stdio(stderr_path, cwd, "/dev/stderr")?
    };

    let (stdout_reader, stdout_writer) = std::os::unix::net::UnixStream::pair()?;
    let (stderr_reader, stderr_writer) = std::os::unix::net::UnixStream::pair()?;
    let stdout_writer = unsafe { OwnedFd::from_raw_fd(stdout_writer.into_raw_fd()) };
    let stderr_writer = unsafe { OwnedFd::from_raw_fd(stderr_writer.into_raw_fd()) };
    command.stdout(Stdio::from(stdout_writer));
    command.stderr(Stdio::from(stderr_writer));

    let stdout_handle = spawn_output_forwarder(
        stdout_reader,
        stdout_target,
        label_output,
        unbuffered,
        "0: ",
    )?;
    let stderr_handle = spawn_output_forwarder(
        stderr_reader,
        stderr_target,
        label_output,
        unbuffered,
        "0: ",
    )?;
    Ok(ForegroundIoState::Streamed(vec![
        stdout_handle,
        stderr_handle,
    ]))
}

fn spawn_output_forwarder(
    reader: std::os::unix::net::UnixStream,
    mut target: std::fs::File,
    label_output: bool,
    unbuffered: bool,
    label_prefix: &'static str,
) -> Result<std::thread::JoinHandle<Result<()>>> {
    Ok(std::thread::spawn(move || {
        if label_output {
            let mut reader = BufReader::new(reader);
            let mut line = Vec::new();
            loop {
                line.clear();
                let bytes = reader.read_until(b'\n', &mut line)?;
                if bytes == 0 {
                    break;
                }
                target.write_all(label_prefix.as_bytes())?;
                target.write_all(&line)?;
                target.flush()?;
            }
            return Ok(());
        }

        let mut reader = reader;
        let mut buf = [0u8; 4096];
        loop {
            let bytes = reader.read(&mut buf)?;
            if bytes == 0 {
                break;
            }
            target.write_all(&buf[..bytes])?;
            if unbuffered {
                target.flush()?;
            }
        }
        if !unbuffered {
            target.flush()?;
        }
        Ok(())
    }))
}

fn setup_local_cgroup(
    config: &AppConfig,
    job_id: i64,
    job: &JobRecord,
    pid: i32,
) -> Result<Option<std::path::PathBuf>> {
    setup_job_cgroup(
        config.cgroup_base.as_deref(),
        job_id,
        job.requested_memory_mb,
        job.requested_cpus,
        job.requested_tasks,
        config.total_cpus,
        pid,
    )
}

fn command_basename(command: &str) -> String {
    Path::new(command)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(command)
        .to_string()
}

fn current_user_name() -> String {
    std::env::var("USER").unwrap_or_else(|_| "unknown".to_string())
}

fn exit_signal(status: &std::process::ExitStatus) -> Option<i32> {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status.signal()
    }
    #[cfg(not(unix))]
    {
        let _ = status;
        None
    }
}

fn allocation_terminal_state(
    exit_code: Option<i32>,
    term_signal: Option<i32>,
    oom_killed: bool,
) -> (JobState, &'static str) {
    if oom_killed {
        return (JobState::OutOfMemory, "OutOfMemory");
    }
    match (exit_code, term_signal) {
        (Some(0), _) => (JobState::Completed, ""),
        (_, Some(_)) => (JobState::Failed, "NonZeroExitCode"),
        (Some(_), _) => (JobState::Failed, "NonZeroExitCode"),
        (None, None) => (JobState::Failed, "NonZeroExitCode"),
    }
}
