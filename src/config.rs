use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crate::error::Result;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub run_dir: PathBuf,
    pub jobs_dir: PathBuf,
    pub socket_path: PathBuf,
    pub db_path: PathBuf,
    pub hostname: String,
    pub scheduler_interval_ms: u64,
    pub cancel_grace_secs: u64,
    pub total_cpus: u32,
    pub total_memory_mb: u64,
    pub total_gpus: u32,
    pub gpu_model: String,
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
        let detected_gpus = detect_gpu_info();

        Self {
            run_dir,
            jobs_dir,
            socket_path,
            db_path,
            hostname: env::var("HOSTNAME").unwrap_or_else(|_| "localhost".to_string()),
            scheduler_interval_ms: 300,
            cancel_grace_secs: 2,
            total_cpus: available_parallelism(),
            total_memory_mb: 16 * 1024,
            total_gpus: env_u32("SLOTD_GPU_COUNT", detected_gpus.count.unwrap_or(1)),
            gpu_model: env::var("SLOTD_GPU_MODEL")
                .ok()
                .or(detected_gpus.model)
                .unwrap_or_else(|| "Generic-GPU".to_string()),
        }
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        fs::create_dir_all(&self.run_dir)?;
        fs::create_dir_all(&self.jobs_dir)?;
        Ok(())
    }

    pub fn has_partition(&self, partition: &str) -> bool {
        matches!(partition, "cpu" | "gpu")
    }

    pub fn default_partition(&self) -> &'static str {
        "gpu"
    }

    pub fn default_gpus_for_partition(&self, partition: &str) -> u32 {
        if partition == "gpu" { 1 } else { 0 }
    }
}

fn available_parallelism() -> u32 {
    std::thread::available_parallelism()
        .map(|count| count.get() as u32)
        .unwrap_or(1)
}

fn env_u32(name: &str, default: u32) -> u32 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

struct DetectedGpuInfo {
    count: Option<u32>,
    model: Option<String>,
}

fn detect_gpu_info() -> DetectedGpuInfo {
    let Ok(output) = Command::new("nvidia-smi")
        .args(["--query-gpu=name", "--format=csv,noheader"])
        .output()
    else {
        return DetectedGpuInfo {
            count: None,
            model: None,
        };
    };

    if !output.status.success() {
        return DetectedGpuInfo {
            count: None,
            model: None,
        };
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let names = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();

    let count = (!names.is_empty()).then_some(names.len() as u32);
    let model = names.first().map(|name| normalize_gpu_name(name));

    DetectedGpuInfo { count, model }
}

fn normalize_gpu_name(value: &str) -> String {
    let trimmed = value.trim().trim_start_matches("NVIDIA ").trim();
    if trimmed.eq_ignore_ascii_case("H200") {
        return "H200".to_string();
    }

    let compact = trimmed
        .replace("GeForce", "")
        .replace("Tesla", "")
        .replace("RTX ", "RTX")
        .replace("GTX ", "GTX")
        .replace("  ", " ");
    compact.split_whitespace().collect::<String>()
}
