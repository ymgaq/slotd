use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use nix::errno::Errno;
use nix::sys::signal::kill;
use nix::unistd::Pid;

use crate::app::config::AppConfig;
use crate::app::error::Result;
use crate::model::job::JobState;
use crate::runtime::cgroup::cgroup_oomed;

pub(crate) fn process_group_alive(pgid: i32) -> Result<bool> {
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

pub(crate) fn wait_for_group_exit(pgid: i32, timeout_secs: u64) -> Result<()> {
    let retries = std::cmp::max(1, timeout_secs * 10);
    for _ in 0..retries {
        if !process_group_alive(pgid)? {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

pub(crate) fn process_group_alive_for_recovery(pgid: i32) -> Result<bool> {
    process_group_alive(pgid)
}

pub(crate) fn recovered_terminal_state(
    status_path: &Option<PathBuf>,
    cgroup_path: Option<&Path>,
) -> (JobState, Option<i32>, &'static str) {
    if cgroup_oomed(cgroup_path) {
        return (JobState::OutOfMemory, None, "OutOfMemory");
    }
    if let Some(exit_code) = status_path
        .as_ref()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|value| value.trim().parse::<i32>().ok())
    {
        return match exit_code {
            0 => (JobState::Completed, Some(0), "Completed"),
            code => (JobState::Failed, Some(code), "NonZeroExitCode"),
        };
    }
    (JobState::Failed, None, "LostAfterRestart")
}

pub(crate) fn job_cgroup_path(config: &AppConfig, job_id: i64) -> Option<PathBuf> {
    config
        .cgroup_base
        .as_ref()
        .map(|base| base.join(format!("slotd-{job_id}")))
}

pub(crate) fn read_process_rss_kb(pid: i32) -> Option<u64> {
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
