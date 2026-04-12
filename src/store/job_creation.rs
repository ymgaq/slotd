use std::path::PathBuf;

use rusqlite::params;

use crate::app::error::{Result, SlotdError};
use crate::model::job::{JobState, SubmitRequest};
use crate::store::support::{default_name, ensure_parent_dir, path_string, script_command};
use crate::submit::sbatch::{expand_output_pattern, parse_array_spec, resolve_log_path};
use crate::util::time::now_ts;

use super::{Store, default_output_pattern};

impl Store {
    pub fn create_job(&self, request: SubmitRequest) -> Result<i64> {
        if request.array_spec.is_some() {
            return self.create_array_jobs(request);
        }
        self.create_job_entry(
            &request,
            None,
            None,
            None,
            None,
            request
                .name
                .clone()
                .unwrap_or_else(|| default_name(&request.script_name)),
        )
    }

    fn create_array_jobs(&self, request: SubmitRequest) -> Result<i64> {
        let spec = parse_array_spec(
            request
                .array_spec
                .as_deref()
                .ok_or_else(|| SlotdError::from("missing array specification"))?,
        )?;
        let base_name = request
            .name
            .clone()
            .unwrap_or_else(|| default_name(&request.script_name));
        let task_count = spec.task_ids.len() as u32;
        let mut array_job_id = None;

        for task_id in spec.task_ids {
            let job_id = self.create_job_entry(
                &request,
                array_job_id,
                Some(task_id),
                Some(task_count),
                spec.limit,
                base_name.clone(),
            )?;
            if array_job_id.is_none() {
                array_job_id = Some(job_id);
                self.conn
                    .execute("UPDATE jobs SET array_job_id = ?1 WHERE id = ?1", [job_id])?;
            }
        }

        array_job_id.ok_or_else(|| SlotdError::from("array specification produced no jobs"))
    }

    fn create_job_entry(
        &self,
        request: &SubmitRequest,
        array_job_id: Option<i64>,
        array_task_id: Option<i32>,
        array_task_count: Option<u32>,
        array_task_limit: Option<u32>,
        resolved_name: String,
    ) -> Result<i64> {
        let submit_time = now_ts();
        let user_name = request.user_name.clone();
        self.conn.execute(
            "INSERT INTO jobs (
                parent_job_id, step_id, held, priority, name, user_name, state, partition, command, cwd, requested_cpus, requested_memory_mb,
                requested_tasks, requested_gpus, allocation_only, dependency,
                array_job_id, array_task_id, array_task_count, array_task_limit, submit_time,
                state_reason, time_limit_secs, begin_time, exclusive, export_env, open_mode, warning_signal, warning_signal_seconds, [constraint], cpu_bind, requeue, requeue_count
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33)",
            params![
                Option::<i64>::None,
                Option::<i64>::None,
                false,
                0i32,
                resolved_name,
                user_name,
                JobState::Pending.as_str(),
                &request.partition,
                request
                    .command_override
                    .clone()
                    .unwrap_or_else(|| script_command(&request.script_name)),
                &request.cwd,
                request.requested_cpus,
                request.requested_memory_mb,
                request.requested_tasks,
                request.requested_gpus,
                request.allocation_only,
                request.dependency.as_deref(),
                array_job_id,
                array_task_id,
                array_task_count.map(|value| value as i64),
                array_task_limit.map(|value| value as i64),
                submit_time,
                "Resources",
                request.time_limit_secs.map(|value| value as i64),
                request.begin_time,
                request.exclusive,
                serde_json::to_string(&request.export_env)?,
                request.open_mode.as_str(),
                request.warning_signal.as_ref().map(|value| value.signal),
                request
                    .warning_signal
                    .as_ref()
                    .map(|value| value.seconds_before_end as i64),
                request.constraint.as_deref(),
                request.cpu_bind.as_deref(),
                request.requeue,
                0i64,
            ],
        )?;

        let job_id = self.conn.last_insert_rowid();
        let job_dir = self.config.jobs_dir.join(job_id.to_string());
        std::fs::create_dir_all(&job_dir)?;

        let script_path = job_dir.join("script.sh");
        std::fs::write(&script_path, &request.script_body)?;

        let default_stdout = expand_output_pattern(
            default_output_pattern(array_task_id),
            job_id,
            &resolved_name,
            &user_name,
            &self.config.hostname,
            array_job_id,
            array_task_id,
        );
        let stdout_path = request
            .stdout_path
            .as_deref()
            .map(|path| {
                expand_output_pattern(
                    path,
                    job_id,
                    &resolved_name,
                    &user_name,
                    &self.config.hostname,
                    array_job_id,
                    array_task_id,
                )
            })
            .unwrap_or(default_stdout);
        let stdout_path = PathBuf::from(resolve_log_path(&request.cwd, &stdout_path));
        let stderr_path = request
            .stderr_path
            .as_deref()
            .map(|path| {
                expand_output_pattern(
                    path,
                    job_id,
                    &resolved_name,
                    &user_name,
                    &self.config.hostname,
                    array_job_id,
                    array_task_id,
                )
            })
            .map(|path| PathBuf::from(resolve_log_path(&request.cwd, &path)))
            .unwrap_or_else(|| stdout_path.clone());
        ensure_parent_dir(&stdout_path)?;
        ensure_parent_dir(&stderr_path)?;

        self.conn.execute(
            "UPDATE jobs
             SET script_path = ?1, stdout_path = ?2, stderr_path = ?3
             WHERE id = ?4",
            params![
                path_string(&script_path),
                path_string(&stdout_path),
                path_string(&stderr_path),
                job_id
            ],
        )?;

        Ok(job_id)
    }
}
