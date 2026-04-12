use std::path::PathBuf;

use crate::app::config::AppConfig;
use crate::app::error::Result;
use crate::model::job::SubmitRequest;
use crate::store::support::{ensure_parent_dir, path_string};
use crate::submit::sbatch::{expand_output_pattern, resolve_log_path};

pub(super) struct JobPaths {
    pub(super) script_path: PathBuf,
    pub(super) stdout_path: PathBuf,
    pub(super) stderr_path: PathBuf,
}

impl JobPaths {
    pub(super) fn script_path_string(&self) -> String {
        path_string(&self.script_path)
    }

    pub(super) fn stdout_path_string(&self) -> String {
        path_string(&self.stdout_path)
    }

    pub(super) fn stderr_path_string(&self) -> String {
        path_string(&self.stderr_path)
    }
}

pub(super) fn prepare_job_paths(
    config: &AppConfig,
    request: &SubmitRequest,
    default_output_pattern: &str,
    resolved_name: &str,
    job_id: i64,
    array_job_id: Option<i64>,
    array_task_id: Option<i32>,
) -> Result<JobPaths> {
    let job_dir = config.jobs_dir.join(job_id.to_string());
    std::fs::create_dir_all(&job_dir)?;

    let script_path = job_dir.join("script.sh");
    std::fs::write(&script_path, &request.script_body)?;

    let stdout_path = resolve_output_path(
        config,
        request,
        request
            .stdout_path
            .as_deref()
            .unwrap_or(default_output_pattern),
        resolved_name,
        job_id,
        array_job_id,
        array_task_id,
    );
    let stderr_path = request
        .stderr_path
        .as_deref()
        .map(|pattern| {
            resolve_output_path(
                config,
                request,
                pattern,
                resolved_name,
                job_id,
                array_job_id,
                array_task_id,
            )
        })
        .unwrap_or_else(|| stdout_path.clone());
    ensure_parent_dir(&stdout_path)?;
    ensure_parent_dir(&stderr_path)?;

    Ok(JobPaths {
        script_path,
        stdout_path,
        stderr_path,
    })
}

fn resolve_output_path(
    config: &AppConfig,
    request: &SubmitRequest,
    pattern: &str,
    resolved_name: &str,
    job_id: i64,
    array_job_id: Option<i64>,
    array_task_id: Option<i32>,
) -> PathBuf {
    let expanded = expand_output_pattern(
        pattern,
        job_id,
        resolved_name,
        &request.user_name,
        &config.hostname,
        array_job_id,
        array_task_id,
    );
    PathBuf::from(resolve_log_path(&request.cwd, &expanded))
}
