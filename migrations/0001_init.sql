CREATE TABLE IF NOT EXISTS jobs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    state TEXT NOT NULL,
    command TEXT NOT NULL,
    cwd TEXT NOT NULL,
    requested_cpus INTEGER NOT NULL,
    requested_memory_mb INTEGER NOT NULL,
    submit_time INTEGER NOT NULL,
    start_time INTEGER,
    end_time INTEGER,
    pid INTEGER,
    pgid INTEGER,
    exit_code INTEGER,
    script_path TEXT NOT NULL DEFAULT '',
    stdout_path TEXT NOT NULL DEFAULT '',
    stderr_path TEXT NOT NULL DEFAULT ''
);
