use crate::error::SlotdError;
use crate::job::{JobRecord, WarningSignal};
use crate::store_support::{parse_export_env_json, parse_gpu_ids};

pub(super) const JOB_SELECT_COLUMNS: &str = "id, parent_job_id, step_id, held, priority, name, user_name, state, partition, command, cwd, requested_cpus, requested_memory_mb,
                    requested_tasks, requested_gpus, allocation_only, dependency, array_job_id,
                    array_task_id, array_task_count, array_task_limit, max_rss_kb,
                    submit_time, start_time, end_time, pid, pgid, exit_code, state_reason, term_signal, time_limit_secs, begin_time, exclusive,
                    assigned_gpus, script_path, stdout_path, stderr_path, export_env, open_mode, warning_signal, warning_signal_seconds, [constraint], cpu_bind, requeue, requeue_count";

pub(super) fn map_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<JobRecord> {
    let state_text: String = row.get(7)?;
    let state = state_text.parse().map_err(|message: String| {
        rusqlite::Error::FromSqlConversionFailure(
            7,
            rusqlite::types::Type::Text,
            Box::new(SlotdError::from(message)),
        )
    })?;
    Ok(JobRecord {
        id: row.get(0)?,
        parent_job_id: row.get(1)?,
        step_id: row.get::<_, Option<i64>>(2)?.map(|value| value as u32),
        held: row.get::<_, bool>(3)?,
        priority: row.get(4)?,
        name: row.get(5)?,
        user_name: row.get(6)?,
        state,
        partition: row.get(8)?,
        command: row.get(9)?,
        cwd: row.get(10)?,
        requested_cpus: row.get(11)?,
        requested_memory_mb: row.get(12)?,
        requested_tasks: row.get(13)?,
        requested_gpus: row.get(14)?,
        allocation_only: row.get::<_, bool>(15)?,
        dependency: row.get(16)?,
        array_job_id: row.get(17)?,
        array_task_id: row.get(18)?,
        array_task_count: row.get::<_, Option<i64>>(19)?.map(|value| value as u32),
        array_task_limit: row.get::<_, Option<i64>>(20)?.map(|value| value as u32),
        max_rss_kb: row.get::<_, Option<i64>>(21)?.map(|value| value as u64),
        submit_time: row.get(22)?,
        start_time: row.get(23)?,
        end_time: row.get(24)?,
        pid: row.get(25)?,
        pgid: row.get(26)?,
        exit_code: row.get(27)?,
        state_reason: row
            .get::<_, String>(28)
            .ok()
            .filter(|value| !value.is_empty()),
        term_signal: row.get(29)?,
        time_limit_secs: row.get::<_, Option<i64>>(30)?.map(|value| value as u64),
        begin_time: row.get(31)?,
        exclusive: row.get::<_, bool>(32)?,
        assigned_gpu_ids: parse_gpu_ids(&row.get::<_, String>(33)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                33,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        script_path: row.get(34)?,
        stdout_path: row.get(35)?,
        stderr_path: row.get(36)?,
        export_env: parse_export_env_json(&row.get::<_, String>(37)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                37,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        open_mode: row
            .get::<_, String>(38)?
            .parse()
            .map_err(|message: String| {
                rusqlite::Error::FromSqlConversionFailure(
                    38,
                    rusqlite::types::Type::Text,
                    Box::new(SlotdError::from(message)),
                )
            })?,
        warning_signal: match (
            row.get::<_, Option<i32>>(39)?,
            row.get::<_, Option<i64>>(40)?,
        ) {
            (Some(signal), Some(seconds_before_end)) => Some(WarningSignal {
                signal,
                seconds_before_end: seconds_before_end as u64,
            }),
            _ => None,
        },
        constraint: row.get(41)?,
        cpu_bind: row.get(42)?,
        requeue: row.get::<_, bool>(43)?,
        requeue_count: row.get::<_, i64>(44)? as u32,
    })
}
