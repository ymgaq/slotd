use std::env;
use std::fs;
use std::path::PathBuf;

use crate::error::Result;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub run_dir: PathBuf,
    pub jobs_dir: PathBuf,
    pub socket_path: PathBuf,
    pub db_path: PathBuf,
    pub scheduler_interval_ms: u64,
    pub cancel_grace_secs: u64,
    pub total_cpus: u32,
    pub total_memory_mb: u64,
}

impl AppConfig {
    pub fn load() -> Self {
        let root_dir = env::var_os("SLOTD_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("var"));
        let run_dir = root_dir.join("run");
        let lib_dir = root_dir.join("lib");
        let jobs_dir = lib_dir.join("jobs");
        let socket_path = run_dir.join("slotd.sock");
        let db_path = lib_dir.join("state.db");

        Self {
            run_dir,
            jobs_dir,
            socket_path,
            db_path,
            scheduler_interval_ms: 300,
            cancel_grace_secs: 2,
            total_cpus: available_parallelism(),
            total_memory_mb: 16 * 1024,
        }
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        fs::create_dir_all(&self.run_dir)?;
        fs::create_dir_all(&self.jobs_dir)?;
        Ok(())
    }
}

fn available_parallelism() -> u32 {
    std::thread::available_parallelism()
        .map(|count| count.get() as u32)
        .unwrap_or(1)
}
