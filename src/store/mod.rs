mod allocation;
mod control_updates;
mod job_creation;
mod listing;
mod lookup;
mod node_reporting;
mod partition_reporting;
mod query;
mod row;
mod schema;
mod state_updates;
mod step_creation;
pub(crate) mod support;
mod updates;

use rusqlite::Connection;

use crate::app::config::AppConfig;
use crate::app::error::Result;
use crate::model::job::{JobRecord, JobState};
use crate::submit::sbatch::default_batch_output_pattern;
pub(crate) use row::{JOB_SELECT_COLUMNS, map_job};
use schema::ensure_compat_schema;

const MIGRATION_SQL: &str = include_str!("../../migrations/0001_init.sql");

pub struct Store {
    conn: Connection,
    config: AppConfig,
}

impl Store {
    pub fn open(config: AppConfig) -> Result<Self> {
        config.ensure_dirs()?;
        let conn = Connection::open(&config.db_path)?;
        conn.execute_batch(MIGRATION_SQL)?;
        let store = Self { conn, config };
        ensure_compat_schema(&store.conn)?;
        Ok(store)
    }

    pub fn config(&self) -> &AppConfig {
        &self.config
    }
}

fn should_auto_requeue(job: &JobRecord, final_state: JobState) -> bool {
    job.parent_job_id.is_none()
        && job.requeue
        && job.requeue_count == 0
        && matches!(
            final_state,
            JobState::Failed | JobState::Timeout | JobState::OutOfMemory
        )
}

fn default_output_pattern(array_task_id: Option<i32>) -> &'static str {
    if array_task_id.is_some() {
        "slurm-%A_%a.out"
    } else {
        default_batch_output_pattern()
    }
}

fn next_step_id_query(conn: &Connection, parent_job_id: i64) -> Result<u32> {
    let current = conn.query_row(
        "SELECT COALESCE(MAX(step_id), -1) FROM jobs WHERE parent_job_id = ?1",
        [parent_job_id],
        |row| row.get::<_, i64>(0),
    )?;
    Ok((current + 1).max(0) as u32)
}

#[cfg(test)]
mod tests {
    use super::Store;
    use crate::app::config::AppConfig;
    use crate::model::job::{JobRecord, JobState, OpenMode, SubmitRequest};
    use crate::store::support::order_pending_jobs;
    use crate::util::time::now_ts;

    fn pending_job(
        id: i64,
        array_job_id: Option<i64>,
        priority: i32,
        submit_time: i64,
    ) -> JobRecord {
        JobRecord {
            id,
            parent_job_id: None,
            step_id: None,
            held: false,
            priority,
            name: format!("job-{id}"),
            user_name: "test".to_string(),
            state: JobState::Pending,
            partition: "cpu".to_string(),
            command: "true".to_string(),
            cwd: "/tmp".to_string(),
            requested_cpus: 1,
            requested_tasks: 1,
            requested_memory_mb: 1,
            requested_gpus: 0,
            allocation_only: false,
            dependency: None,
            array_job_id,
            array_task_id: None,
            array_task_count: None,
            array_task_limit: None,
            max_rss_kb: None,
            submit_time,
            start_time: None,
            end_time: None,
            pid: None,
            pgid: None,
            exit_code: None,
            state_reason: None,
            term_signal: None,
            time_limit_secs: None,
            begin_time: None,
            exclusive: false,
            assigned_gpu_ids: Vec::new(),
            script_path: String::new(),
            stdout_path: String::new(),
            stderr_path: String::new(),
            constraint: None,
            cpu_bind: None,
            export_env: Vec::new(),
            open_mode: OpenMode::Truncate,
            warning_signal: None,
            requeue: false,
            requeue_count: 0,
        }
    }

    #[test]
    fn scheduler_interleaves_array_groups() {
        let ordered = order_pending_jobs(vec![
            pending_job(1, Some(1), 0, 0),
            pending_job(2, Some(1), 0, 1),
            pending_job(3, Some(3), 0, 2),
            pending_job(4, Some(3), 0, 3),
        ]);
        let ids = ordered.into_iter().map(|job| job.id).collect::<Vec<_>>();
        assert_eq!(ids, vec![1, 3, 2, 4]);
    }

    #[test]
    fn scheduler_prefers_higher_explicit_priority() {
        let older = pending_job(10, None, 0, 0);
        let newer = pending_job(11, None, 100, 100);
        let ordered = order_pending_jobs(vec![older, newer]);
        assert_eq!(ordered[0].id, 11);
    }

    #[test]
    fn phase5_requeues_failed_job_once() {
        let root = std::env::temp_dir().join(format!(
            "slotd-phase5-requeue-{}-{}",
            std::process::id(),
            now_ts()
        ));
        std::fs::create_dir_all(&root).expect("create temp root");
        unsafe {
            std::env::set_var("SLOTD_ROOT", &root);
        }
        let store = Store::open(AppConfig::load()).expect("open store");
        let request = SubmitRequest {
            name: Some("demo".to_string()),
            user_name: "test".to_string(),
            partition: store.config().default_partition().to_string(),
            cwd: root.to_string_lossy().to_string(),
            script_name: "job.sh".to_string(),
            script_body: "#!/usr/bin/env bash\nexit 1\n".to_string(),
            command_override: None,
            requested_cpus: 1,
            requested_tasks: 1,
            requested_memory_mb: 64,
            requested_gpus: 0,
            allocation_only: false,
            dependency: None,
            array_spec: None,
            time_limit_secs: None,
            begin_time: None,
            exclusive: false,
            stdout_path: None,
            stderr_path: None,
            constraint: None,
            cpu_bind: None,
            export_env: Vec::new(),
            open_mode: OpenMode::Truncate,
            warning_signal: None,
            requeue: true,
        };

        let job_id = store.create_job(request).expect("create job");
        let job = store
            .mark_finished(
                job_id,
                JobState::Failed,
                Some(1),
                None,
                Some("NonZeroExitCode"),
            )
            .expect("first finish");
        assert_eq!(job.state, JobState::Pending);
        assert_eq!(job.state_reason.as_deref(), Some("Requeued"));
        assert_eq!(job.requeue_count, 1);
        assert_eq!(job.exit_code, None);

        let job = store
            .mark_finished(
                job_id,
                JobState::Failed,
                Some(1),
                None,
                Some("NonZeroExitCode"),
            )
            .expect("second finish");
        assert_eq!(job.state, JobState::Failed);
        assert_eq!(job.requeue_count, 1);

        std::fs::remove_dir_all(&root).expect("remove temp root");
        unsafe {
            std::env::remove_var("SLOTD_ROOT");
        }
    }

    #[test]
    fn phase5_does_not_requeue_cancelled_job() {
        let root = std::env::temp_dir().join(format!(
            "slotd-phase5-cancel-{}-{}",
            std::process::id(),
            now_ts()
        ));
        std::fs::create_dir_all(&root).expect("create temp root");
        unsafe {
            std::env::set_var("SLOTD_ROOT", &root);
        }
        let store = Store::open(AppConfig::load()).expect("open store");
        let request = SubmitRequest {
            name: Some("demo".to_string()),
            user_name: "test".to_string(),
            partition: store.config().default_partition().to_string(),
            cwd: root.to_string_lossy().to_string(),
            script_name: "job.sh".to_string(),
            script_body: "#!/usr/bin/env bash\nsleep 1\n".to_string(),
            command_override: None,
            requested_cpus: 1,
            requested_tasks: 1,
            requested_memory_mb: 64,
            requested_gpus: 0,
            allocation_only: false,
            dependency: None,
            array_spec: None,
            time_limit_secs: None,
            begin_time: None,
            exclusive: false,
            stdout_path: None,
            stderr_path: None,
            constraint: None,
            cpu_bind: None,
            export_env: Vec::new(),
            open_mode: OpenMode::Truncate,
            warning_signal: None,
            requeue: true,
        };

        let job_id = store.create_job(request).expect("create job");
        let job = store
            .mark_finished(
                job_id,
                JobState::Cancelled,
                None,
                Some(15),
                Some("CancelledByUser"),
            )
            .expect("finish");
        assert_eq!(job.state, JobState::Cancelled);
        assert_eq!(job.requeue_count, 0);

        std::fs::remove_dir_all(&root).expect("remove temp root");
        unsafe {
            std::env::remove_var("SLOTD_ROOT");
        }
    }
}
