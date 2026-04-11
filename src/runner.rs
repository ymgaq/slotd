use std::collections::HashMap;
use std::fs::{self, File};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::fs::OpenOptions;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use nix::errno::Errno;
use nix::sched::{CpuSet, sched_setaffinity};
use nix::sys::signal::{Signal, kill, killpg};
use nix::unistd::{Pid, setsid};

use crate::config::AppConfig;
use crate::error::Result;
use crate::job::{JobRecord, JobState, OpenMode};
use crate::store::Store;

pub struct RunningJob {
    pub pgid: i32,
    pub pid: i32,
    pub cgroup_path: Option<PathBuf>,
    pub status_path: Option<PathBuf>,
    pub warning_signal_sent: bool,
    handle: JobHandle,
}

enum JobHandle {
    Child(Child),
    Adopted,
}

pub struct Runner {
    jobs: HashMap<i64, RunningJob>,
}

impl Runner {
    pub fn new() -> Self {
        Self {
            jobs: HashMap::new(),
        }
    }

    pub fn launch(&mut self, store: &Store, job: &JobRecord) -> Result<()> {
        let stdout = open_output_file(&job.stdout_path, job.open_mode)?;
        let stderr = if job.stderr_path == job.stdout_path {
            stdout.try_clone()?
        } else {
            open_output_file(&job.stderr_path, job.open_mode)?
        };
        let assigned_gpu_ids = if store.config().is_gpu_partition(&job.partition) {
            store.allocate_gpu_ids(job.requested_gpus)?
        } else {
            Vec::new()
        };

        let status_path = job_status_path(job)
            .ok_or_else(|| crate::error::SlotdError::from("missing script path for daemon job"))?;
        let wrapper_path = job_wrapper_path(job);
        if !wrapper_path.exists() {
            std::fs::write(
                &wrapper_path,
                format!(
                    "#!/usr/bin/env bash\n/bin/bash {} \ncode=$?\nprintf '%s\\n' \"$code\" > {}\nexit \"$code\"\n",
                    shell_quote_path(&job.script_path),
                    shell_quote_path(&status_path.to_string_lossy())
                ),
            )?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(&wrapper_path)?.permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(&wrapper_path, perms)?;
            }
        }

        let mut command = Command::new("/bin/bash");
        command.arg(&wrapper_path);
        command.current_dir(&job.cwd);
        command.stdin(Stdio::null());
        command.stdout(Stdio::from(stdout));
        command.stderr(Stdio::from(stderr));
        command.envs(job.export_env.iter().cloned());
        command.env("SLURM_JOB_ID", job.id.to_string());
        command.env("SLURM_JOB_NAME", &job.name);
        command.env("SLURM_JOB_PARTITION", &job.partition);
        command.env("SLURM_JOB_NODELIST", store.config().hostname.clone());
        command.env("SLURM_SUBMIT_DIR", &job.cwd);
        command.env("SLURM_NTASKS", job.requested_tasks.to_string());
        command.env("SLURM_CPUS_PER_TASK", job.requested_cpus.to_string());
        if let Some(array_job_id) = job.array_job_id {
            command.env("SLURM_ARRAY_JOB_ID", array_job_id.to_string());
        }
        if let Some(array_task_id) = job.array_task_id {
            command.env("SLURM_ARRAY_TASK_ID", array_task_id.to_string());
        }
        if !assigned_gpu_ids.is_empty() {
            command.env("CUDA_VISIBLE_DEVICES", join_gpu_ids(&assigned_gpu_ids));
        }
        let cpu_bind = resolve_cpu_bind_ids(
            job.cpu_bind.as_deref(),
            store.config().total_cpus,
            job.requested_cpus.saturating_mul(job.requested_tasks).max(1),
        )?;
        // Create a dedicated process group so scancel can terminate the whole tree.
        unsafe {
            command.pre_exec(|| {
                setsid().map_err(std::io::Error::other)?;
                Ok(())
            });
        }
        if let Some(cpu_ids) = cpu_bind {
            unsafe {
                command.pre_exec(move || {
                    apply_cpu_affinity(&cpu_ids).map_err(std::io::Error::other)?;
                    Ok(())
                });
            }
        }

        let child = command.spawn()?;
        let pid = child.id() as i32;
        let pgid = pid;
        let cgroup_path = setup_job_cgroup(store.config(), job, pid)?;
        store.mark_running(job.id, pid, pgid, &assigned_gpu_ids)?;

        self.jobs.insert(
            job.id,
            RunningJob {
                pgid,
                pid,
                cgroup_path,
                status_path: Some(status_path),
                warning_signal_sent: false,
                handle: JobHandle::Child(child),
            },
        );
        Ok(())
    }

    pub fn adopt(&mut self, config: &AppConfig, job: &JobRecord) {
        let pgid = job.pgid.or(job.pid).unwrap_or_default();
        if pgid <= 0 {
            return;
        }

        self.jobs.insert(
            job.id,
            RunningJob {
                pgid,
                pid: job.pid.unwrap_or_default(),
                cgroup_path: job_cgroup_path(config, job.id),
                status_path: job_status_path(job),
                warning_signal_sent: false,
                handle: JobHandle::Adopted,
            },
        );
    }

    pub fn reconcile_adopted(&mut self, store: &Store) -> Result<()> {
        let mut finished = Vec::new();
        for (&job_id, running) in &self.jobs {
            if let JobHandle::Adopted = running.handle {
                if !process_group_alive(running.pgid)? {
                    let (state, exit_code, reason) =
                        recovered_terminal_state(&running.status_path, running.cgroup_path.as_deref());
                    store.mark_finished(job_id, state, exit_code, None, Some(reason))?;
                    cleanup_cgroup(running.cgroup_path.as_deref());
                    finished.push(job_id);
                }
            }
        }

        for job_id in finished {
            self.jobs.remove(&job_id);
        }

        Ok(())
    }

    pub fn poll(&mut self, store: &Store) -> Result<()> {
        let mut finished = Vec::new();
        for (&job_id, running) in &mut self.jobs {
            if running.pid > 0 {
                if let Some(max_rss_kb) = read_process_rss_kb(running.pid) {
                    store.update_max_rss(job_id, max_rss_kb)?;
                }
            }
            match &mut running.handle {
                JobHandle::Child(child) => {
                    if let Some(status) = child.try_wait()? {
                        let exit_code = status.code();
                        let term_signal = exit_signal(&status);
                        let (state, reason) = terminal_state(
                            exit_code,
                            term_signal,
                            cgroup_oomed(running.cgroup_path.as_deref()),
                        );
                        store.mark_finished(job_id, state, exit_code, term_signal, Some(reason))?;
                        cleanup_cgroup(running.cgroup_path.as_deref());
                        finished.push(job_id);
                    }
                }
                JobHandle::Adopted => {}
            }
        }

        for job_id in finished {
            self.jobs.remove(&job_id);
        }

        Ok(())
    }

    pub fn enforce_timeouts(&mut self, config: &AppConfig, store: &Store) -> Result<()> {
        let now = now_ts();
        let timed_out = self
            .jobs
            .keys()
            .copied()
            .filter_map(|job_id| {
                let job = store.get_job(job_id).ok().flatten()?;
                let start = job.start_time?;
                let limit = job.time_limit_secs?;
                (job.state == JobState::Running && now >= start.saturating_add(limit as i64))
                    .then_some(job_id)
            })
            .collect::<Vec<_>>();

        for job_id in timed_out {
            self.terminate_job(config, store, job_id, JobState::Timeout, "TimeLimit")?;
        }

        let jobs_to_warn = self
            .jobs
            .iter()
            .filter_map(|(&job_id, running)| (!running.warning_signal_sent).then_some(job_id))
            .collect::<Vec<_>>();
        for job_id in jobs_to_warn {
            let Some(job) = store.get_job(job_id)? else {
                continue;
            };
            let Some(start) = job.start_time else {
                continue;
            };
            let Some(limit) = job.time_limit_secs else {
                continue;
            };
            let Some(warning_signal) = &job.warning_signal else {
                continue;
            };
            let deadline = start.saturating_add(limit as i64);
            let warn_at = deadline.saturating_sub(warning_signal.seconds_before_end as i64);
            if now >= warn_at && now < deadline {
                self.signal_job(job_id, warning_signal.signal)?;
                if let Some(running) = self.jobs.get_mut(&job_id) {
                    running.warning_signal_sent = true;
                }
            }
        }

        Ok(())
    }

    pub fn cancel(&mut self, config: &AppConfig, store: &Store, job_id: i64) -> Result<bool> {
        if store.cancel_pending_job(job_id)? {
            return Ok(true);
        }

        if !self.jobs.contains_key(&job_id) {
            return Ok(false);
        }

        self.terminate_job(
            config,
            store,
            job_id,
            JobState::Cancelled,
            "CancelledByUser",
        )?;
        Ok(true)
    }

    pub fn signal_job(&self, job_id: i64, signal: i32) -> Result<bool> {
        let Some(running) = self.jobs.get(&job_id) else {
            return Ok(false);
        };
        let signal = Signal::try_from(signal)
            .map_err(|_| crate::error::SlotdError::from(format!("unsupported signal: {signal}")))?;
        let _ = killpg(Pid::from_raw(running.pgid), signal);
        Ok(true)
    }
}

impl Runner {
    fn terminate_job(
        &mut self,
        config: &AppConfig,
        store: &Store,
        job_id: i64,
        final_state: JobState,
        final_reason: &str,
    ) -> Result<()> {
        let Some(running) = self.jobs.remove(&job_id) else {
            return Ok(());
        };

        store.mark_state(job_id, JobState::Completing, Some(final_reason))?;

        let pgid = Pid::from_raw(running.pgid);
        let _ = killpg(pgid, Signal::SIGTERM);
        thread::sleep(Duration::from_secs(config.cancel_grace_secs));

        let (exit_code, term_signal) = match running.handle {
            JobHandle::Child(mut child) => {
                if let Some(status) = child.try_wait()? {
                    (status.code(), exit_signal(&status))
                } else {
                    let _ = killpg(pgid, Signal::SIGKILL);
                    let status = child.wait()?;
                    (
                        status.code(),
                        exit_signal(&status).or(Some(Signal::SIGKILL as i32)),
                    )
                }
            }
            JobHandle::Adopted => {
                if process_group_alive(running.pgid)? {
                    let _ = killpg(pgid, Signal::SIGKILL);
                    wait_for_group_exit(running.pgid, config.cancel_grace_secs)?;
                }
                (None, Some(Signal::SIGKILL as i32))
            }
        };

        let (final_state, final_reason) = if cgroup_oomed(running.cgroup_path.as_deref()) {
            (JobState::OutOfMemory, "OutOfMemory")
        } else {
            (final_state, final_reason)
        };
        store.mark_finished(
            job_id,
            final_state,
            exit_code,
            term_signal,
            Some(final_reason),
        )?;
        cleanup_cgroup(running.cgroup_path.as_deref());
        Ok(())
    }
}

fn terminal_state(
    exit_code: Option<i32>,
    term_signal: Option<i32>,
    cgroup_oom: bool,
) -> (JobState, &'static str) {
    if cgroup_oom {
        return (JobState::OutOfMemory, "OutOfMemory");
    }
    match (exit_code, term_signal) {
        (Some(0), None) => (JobState::Completed, "Completed"),
        (Some(_), None) => (JobState::Failed, "NonZeroExitCode"),
        (_, Some(_)) => (JobState::Failed, "Signal"),
        _ => (JobState::Failed, "UnknownFailure"),
    }
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

fn process_group_alive(pgid: i32) -> Result<bool> {
    if pgid <= 0 {
        return Ok(false);
    }

    match kill(Pid::from_raw(-pgid), None) {
        Ok(()) => Ok(true),
        Err(Errno::EPERM) => Ok(true),
        Err(Errno::ESRCH) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn wait_for_group_exit(pgid: i32, timeout_secs: u64) -> Result<()> {
    let retries = std::cmp::max(1, timeout_secs * 10);
    for _ in 0..retries {
        if !process_group_alive(pgid)? {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

fn now_ts() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

pub fn process_group_alive_for_recovery(pgid: i32) -> Result<bool> {
    process_group_alive(pgid)
}

fn join_gpu_ids(ids: &[u32]) -> String {
    ids.iter().map(u32::to_string).collect::<Vec<_>>().join(",")
}

fn open_output_file(path: &str, open_mode: OpenMode) -> Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).write(true);
    match open_mode {
        OpenMode::Append => {
            options.append(true);
        }
        OpenMode::Truncate => {
            options.truncate(true);
        }
    }
    Ok(options.open(path)?)
}

fn resolve_cpu_bind_ids(
    value: Option<&str>,
    total_cpus: u32,
    requested_cpus: u32,
) -> Result<Option<Vec<usize>>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let normalized = value.to_ascii_lowercase();
    if normalized == "none" {
        return Ok(None);
    }
    if normalized == "cores" {
        let limit = requested_cpus.min(total_cpus).max(1);
        return Ok(Some((0..limit as usize).collect()));
    }
    if let Some(list) = normalized.strip_prefix("map_cpu:") {
        let mut cpus = Vec::new();
        for part in list.split(',').map(str::trim).filter(|part| !part.is_empty()) {
            let cpu = part.parse::<usize>().map_err(|_| {
                crate::error::SlotdError::from(format!("invalid cpu-bind cpu id: {part}"))
            })?;
            if cpu >= total_cpus as usize {
                return Err(crate::error::SlotdError::from(format!(
                    "cpu-bind cpu id {cpu} exceeds available CPUs"
                )));
            }
            cpus.push(cpu);
        }
        if cpus.is_empty() {
            return Err(crate::error::SlotdError::from(
                "cpu-bind map_cpu requires at least one CPU id",
            ));
        }
        cpus.sort_unstable();
        cpus.dedup();
        return Ok(Some(cpus));
    }
    Err(crate::error::SlotdError::from(format!(
        "unsupported cpu-bind value: {value}; supported: none, cores, map_cpu:<ids>"
    )))
}

fn apply_cpu_affinity(cpu_ids: &[usize]) -> Result<()> {
    let mut cpu_set = CpuSet::new();
    for &cpu_id in cpu_ids {
        cpu_set
            .set(cpu_id)
            .map_err(|error| crate::error::SlotdError::from(error.to_string()))?;
    }
    sched_setaffinity(Pid::from_raw(0), &cpu_set)
        .map_err(|error| crate::error::SlotdError::from(error.to_string()))
}

fn job_status_path(job: &JobRecord) -> Option<PathBuf> {
    if job.script_path.is_empty() {
        return None;
    }
    Some(
        Path::new(&job.script_path)
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("exit_status"),
    )
}

fn job_wrapper_path(job: &JobRecord) -> PathBuf {
    Path::new(&job.script_path)
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("runner.sh")
}

fn recovered_terminal_state(
    status_path: &Option<PathBuf>,
    cgroup_path: Option<&Path>,
) -> (JobState, Option<i32>, &'static str) {
    if cgroup_oomed(cgroup_path) {
        return (JobState::OutOfMemory, None, "OutOfMemory");
    }
    if let Some(exit_code) = status_path
        .as_ref()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|value| value.trim().parse::<i32>().ok())
    {
        return match exit_code {
            0 => (JobState::Completed, Some(0), "Completed"),
            code => (JobState::Failed, Some(code), "NonZeroExitCode"),
        };
    }
    (JobState::Failed, None, "LostAfterRestart")
}

fn shell_quote_path(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn setup_job_cgroup(config: &AppConfig, job: &JobRecord, pid: i32) -> Result<Option<PathBuf>> {
    let Some(path) = job_cgroup_path(config, job.id) else {
        return Ok(None);
    };

    fs::create_dir_all(&path)?;
    let memory_bytes = job.requested_memory_mb.saturating_mul(1024 * 1024);
    fs::write(path.join("memory.max"), memory_bytes.to_string())?;

    let total_requested_cpus = job
        .requested_cpus
        .saturating_mul(job.requested_tasks)
        .max(1);
    let quota = 100_000u64
        .saturating_mul(total_requested_cpus as u64)
        .checked_div(config.total_cpus.max(1) as u64)
        .unwrap_or(100_000)
        .max(1);
    fs::write(path.join("cpu.max"), format!("{quota} 100000"))?;
    fs::write(path.join("cgroup.procs"), pid.to_string())?;
    Ok(Some(path))
}

fn job_cgroup_path(config: &AppConfig, job_id: i64) -> Option<PathBuf> {
    config
        .cgroup_base
        .as_ref()
        .map(|base| base.join(format!("slotd-{job_id}")))
}

fn cleanup_cgroup(path: Option<&Path>) {
    let Some(path) = path else {
        return;
    };
    let _ = fs::remove_dir(path);
}

fn cgroup_oomed(path: Option<&Path>) -> bool {
    let Some(path) = path else {
        return false;
    };
    let Ok(contents) = fs::read_to_string(path.join("memory.events")) else {
        return false;
    };
    contents.lines().any(|line| {
        let mut parts = line.split_whitespace();
        matches!(parts.next(), Some("oom_kill") | Some("oom"))
            && parts
                .next()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(0)
                > 0
    })
}

fn read_process_rss_kb(pid: i32) -> Option<u64> {
    let contents = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    contents.lines().find_map(|line| {
        if !(line.starts_with("VmHWM:") || line.starts_with("VmRSS:")) {
            return None;
        }
        line.split_whitespace()
            .nth(1)
            .and_then(|value| value.parse::<u64>().ok())
    })
}
