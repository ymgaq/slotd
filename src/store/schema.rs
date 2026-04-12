use rusqlite::Connection;

use crate::error::Result;

pub(super) fn ensure_compat_schema(conn: &Connection) -> Result<()> {
    ensure_column(
        conn,
        "jobs",
        "parent_job_id",
        "ALTER TABLE jobs ADD COLUMN parent_job_id INTEGER",
    )?;
    ensure_column(
        conn,
        "jobs",
        "step_id",
        "ALTER TABLE jobs ADD COLUMN step_id INTEGER",
    )?;
    ensure_column(
        conn,
        "jobs",
        "held",
        "ALTER TABLE jobs ADD COLUMN held INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        conn,
        "jobs",
        "priority",
        "ALTER TABLE jobs ADD COLUMN priority INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        conn,
        "jobs",
        "partition",
        "ALTER TABLE jobs ADD COLUMN partition TEXT NOT NULL DEFAULT 'cpu'",
    )?;
    ensure_column(
        conn,
        "jobs",
        "user_name",
        "ALTER TABLE jobs ADD COLUMN user_name TEXT NOT NULL DEFAULT 'unknown'",
    )?;
    ensure_column(
        conn,
        "jobs",
        "requested_tasks",
        "ALTER TABLE jobs ADD COLUMN requested_tasks INTEGER NOT NULL DEFAULT 1",
    )?;
    ensure_column(
        conn,
        "jobs",
        "requested_gpus",
        "ALTER TABLE jobs ADD COLUMN requested_gpus INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        conn,
        "jobs",
        "allocation_only",
        "ALTER TABLE jobs ADD COLUMN allocation_only INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        conn,
        "jobs",
        "assigned_gpus",
        "ALTER TABLE jobs ADD COLUMN assigned_gpus TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        conn,
        "jobs",
        "state_reason",
        "ALTER TABLE jobs ADD COLUMN state_reason TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        conn,
        "jobs",
        "term_signal",
        "ALTER TABLE jobs ADD COLUMN term_signal INTEGER",
    )?;
    ensure_column(
        conn,
        "jobs",
        "time_limit_secs",
        "ALTER TABLE jobs ADD COLUMN time_limit_secs INTEGER",
    )?;
    ensure_column(
        conn,
        "jobs",
        "dependency",
        "ALTER TABLE jobs ADD COLUMN dependency TEXT",
    )?;
    ensure_column(
        conn,
        "jobs",
        "array_job_id",
        "ALTER TABLE jobs ADD COLUMN array_job_id INTEGER",
    )?;
    ensure_column(
        conn,
        "jobs",
        "array_task_id",
        "ALTER TABLE jobs ADD COLUMN array_task_id INTEGER",
    )?;
    ensure_column(
        conn,
        "jobs",
        "array_task_count",
        "ALTER TABLE jobs ADD COLUMN array_task_count INTEGER",
    )?;
    ensure_column(
        conn,
        "jobs",
        "array_task_limit",
        "ALTER TABLE jobs ADD COLUMN array_task_limit INTEGER",
    )?;
    ensure_column(
        conn,
        "jobs",
        "max_rss_kb",
        "ALTER TABLE jobs ADD COLUMN max_rss_kb INTEGER",
    )?;
    ensure_column(
        conn,
        "jobs",
        "begin_time",
        "ALTER TABLE jobs ADD COLUMN begin_time INTEGER",
    )?;
    ensure_column(
        conn,
        "jobs",
        "exclusive",
        "ALTER TABLE jobs ADD COLUMN exclusive INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        conn,
        "jobs",
        "export_env",
        "ALTER TABLE jobs ADD COLUMN export_env TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        conn,
        "jobs",
        "open_mode",
        "ALTER TABLE jobs ADD COLUMN open_mode TEXT NOT NULL DEFAULT 'truncate'",
    )?;
    ensure_column(
        conn,
        "jobs",
        "warning_signal",
        "ALTER TABLE jobs ADD COLUMN warning_signal INTEGER",
    )?;
    ensure_column(
        conn,
        "jobs",
        "warning_signal_seconds",
        "ALTER TABLE jobs ADD COLUMN warning_signal_seconds INTEGER",
    )?;
    ensure_column(
        conn,
        "jobs",
        "constraint",
        "ALTER TABLE jobs ADD COLUMN [constraint] TEXT",
    )?;
    ensure_column(
        conn,
        "jobs",
        "cpu_bind",
        "ALTER TABLE jobs ADD COLUMN cpu_bind TEXT",
    )?;
    ensure_column(
        conn,
        "jobs",
        "requeue",
        "ALTER TABLE jobs ADD COLUMN requeue INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        conn,
        "jobs",
        "requeue_count",
        "ALTER TABLE jobs ADD COLUMN requeue_count INTEGER NOT NULL DEFAULT 0",
    )?;
    Ok(())
}

fn ensure_column(conn: &Connection, table: &str, column: &str, alter_sql: &str) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let mut exists = false;
    for entry in columns {
        if entry? == column {
            exists = true;
            break;
        }
    }

    if !exists {
        conn.execute_batch(alter_sql)?;
    }
    Ok(())
}
