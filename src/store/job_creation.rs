use rusqlite::params;

use crate::app::error::Result;
use crate::model::job::{JobState, SubmitRequest};
use crate::store::job_paths::prepare_job_paths;
use crate::store::support::{default_name, script_command};
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

    pub(super) fn create_job_entry(
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
                &resolved_name,
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
        let default_stdout = default_output_pattern(array_task_id);
        let paths = prepare_job_paths(
            &self.config,
            request,
            default_stdout,
            &resolved_name,
            job_id,
            array_job_id,
            array_task_id,
        )?;

        self.conn.execute(
            "UPDATE jobs
             SET script_path = ?1, stdout_path = ?2, stderr_path = ?3
             WHERE id = ?4",
            params![
                paths.script_path_string(),
                paths.stdout_path_string(),
                paths.stderr_path_string(),
                job_id
            ],
        )?;

        Ok(job_id)
    }
}
