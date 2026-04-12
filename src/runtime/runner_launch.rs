use std::fs::File;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::app::error::Result;
use crate::model::job::{JobRecord, OpenMode};
use crate::runtime::launch::{LaunchCommand, build_multitask_launcher};
use crate::runtime::launch_support::{configure_detached_launch, setup_job_cgroup_for_launch};
use crate::runtime::slurm_env::apply_slurm_env;
use crate::store::Store;
use crate::store::support::join_gpu_ids;

use crate::runtime::runner::{JobHandle, RunningJob};

pub(crate) fn launch_running_job(store: &Store, job: &JobRecord) -> Result<RunningJob> {
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
        .ok_or_else(|| crate::app::error::SlotdError::from("missing script path for daemon job"))?;
    let wrapper_path = job_wrapper_path(job);
    std::fs::write(
        &wrapper_path,
        format!(
            "{}\ncode=$?\nprintf '%s\\n' \"$code\" > {}\nexit \"$code\"\n",
            build_multitask_launcher(
                LaunchCommand::Script(Path::new(&job.script_path)),
                job.requested_tasks,
                false,
            ),
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

    let mut command = Command::new("/bin/bash");
    command.arg(&wrapper_path);
    command.current_dir(&job.cwd);
    command.stdin(Stdio::null());
    command.stdout(Stdio::from(stdout));
    command.stderr(Stdio::from(stderr));
    command.envs(job.export_env.iter().cloned());
    apply_slurm_env(&mut command, store.config(), job);
    if !assigned_gpu_ids.is_empty() {
        command.env("CUDA_VISIBLE_DEVICES", join_gpu_ids(&assigned_gpu_ids));
    }
    configure_detached_launch(
        &mut command,
        job.cpu_bind.as_deref(),
        store.config().total_cpus,
        job.requested_cpus
            .saturating_mul(job.requested_tasks)
            .max(1),
    )?;

    let child = command.spawn()?;
    let pid = child.id() as i32;
    let pgid = pid;
    let cgroup_path = setup_job_cgroup_for_launch(store.config(), job.id, job, pid)?;
    store.mark_running(job.id, pid, pgid, &assigned_gpu_ids)?;

    Ok(RunningJob {
        pgid,
        pid,
        cgroup_path,
        status_path: Some(status_path),
        warning_signal_sent: false,
        handle: JobHandle::Child(child),
    })
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

pub(crate) fn job_status_path(job: &JobRecord) -> Option<PathBuf> {
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

fn shell_quote_path(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}
