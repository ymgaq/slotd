use std::env;
use std::fs;
use std::path::Path;
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
    pub notify_command: Option<String>,
    pub cgroup_base: Option<PathBuf>,
    features: Vec<String>,
    cpu_partitions: Vec<String>,
    gpu_partitions: Vec<String>,
    default_partition: String,
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
        let total_gpus = env_u32("SLOTD_GPU_COUNT", detected_gpus.count.unwrap_or(0));
        let cpu_partitions = env_partition_list("SLOTD_CPU_PARTITIONS", &["cpu"]);
        let gpu_partitions = if total_gpus > 0 {
            env_partition_list("SLOTD_GPU_PARTITIONS", &["gpu"])
        } else {
            Vec::new()
        };
        let features = detect_features(total_gpus, &detected_gpus.model);
        let default_partition = gpu_partitions
            .first()
            .cloned()
            .or_else(|| cpu_partitions.first().cloned())
            .unwrap_or_else(|| "cpu".to_string());

        Self {
            run_dir,
            jobs_dir,
            socket_path,
            db_path,
            hostname: env::var("HOSTNAME").unwrap_or_else(|_| "localhost".to_string()),
            scheduler_interval_ms: 300,
            cancel_grace_secs: 2,
            total_cpus: available_parallelism(),
            total_memory_mb: detect_total_memory_mb().unwrap_or(16 * 1024),
            total_gpus,
            gpu_model: env::var("SLOTD_GPU_MODEL")
                .ok()
                .or(detected_gpus.model)
                .unwrap_or_else(|| "Generic-GPU".to_string()),
            notify_command: env::var("SLOTD_NOTIFY_CMD")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            cgroup_base: env::var_os("SLOTD_CGROUP_BASE").map(PathBuf::from),
            features,
            cpu_partitions,
            gpu_partitions,
            default_partition,
        }
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        fs::create_dir_all(&self.run_dir)?;
        fs::create_dir_all(&self.jobs_dir)?;
        Ok(())
    }

    pub fn has_partition(&self, partition: &str) -> bool {
        self.active_partitions()
            .iter()
            .any(|name| name == partition)
    }

    pub fn default_partition(&self) -> &str {
        &self.default_partition
    }

    pub fn active_partitions(&self) -> Vec<String> {
        self.cpu_partitions
            .iter()
            .chain(self.gpu_partitions.iter())
            .cloned()
            .collect()
    }

    pub fn default_gpus_for_partition(&self, partition: &str) -> u32 {
        if self.is_gpu_partition(partition) {
            1
        } else {
            0
        }
    }

    pub fn is_gpu_partition(&self, partition: &str) -> bool {
        self.gpu_partitions.iter().any(|name| name == partition)
    }

    pub fn available_features(&self) -> &[String] {
        &self.features
    }

    pub fn format_features(&self) -> String {
        self.features.join(",")
    }

    pub fn matches_constraint(&self, constraint: &str, partition: &str) -> bool {
        if constraint.trim().is_empty() {
            return true;
        }
        let features = self.available_features();
        parse_constraint_terms(constraint).iter().all(|term| {
            term.eq_ignore_ascii_case(partition)
                || features
                    .iter()
                    .any(|feature| feature.eq_ignore_ascii_case(term))
        })
    }
}

fn available_parallelism() -> u32 {
    std::thread::available_parallelism()
        .map(|count| count.get() as u32)
        .unwrap_or(1)
}

fn detect_total_memory_mb() -> Option<u64> {
    fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|contents| parse_total_memory_mb(&contents))
}

fn parse_total_memory_mb(contents: &str) -> Option<u64> {
    contents.lines().find_map(|line| {
        let value = line.strip_prefix("MemTotal:")?;
        let mut parts = value.split_whitespace();
        let kb = parts.next()?.parse::<u64>().ok()?;
        match parts.next() {
            Some(unit) if unit.eq_ignore_ascii_case("kB") => Some(kb / 1024),
            _ => None,
        }
    })
}

fn env_u32(name: &str, default: u32) -> u32 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_partition_list(name: &str, defaults: &[&str]) -> Vec<String> {
    env::var(name)
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|values| !values.is_empty())
        .unwrap_or_else(|| defaults.iter().map(|value| value.to_string()).collect())
}

fn detect_features(total_gpus: u32, gpu_model: &Option<String>) -> Vec<String> {
    let mut features = env::var("SLOTD_FEATURES")
        .ok()
        .map(|value| parse_constraint_terms(&value))
        .unwrap_or_default();
    features.push("cpu".to_string());
    if total_gpus > 0 {
        features.push("gpu".to_string());
    }
    if let Some(model) = gpu_model {
        let normalized = normalize_feature_name(model);
        if !normalized.is_empty() {
            features.push(normalized);
        }
    }
    features.sort();
    features.dedup();
    features
}

fn parse_constraint_terms(value: &str) -> Vec<String> {
    value
        .split([',', '&'])
        .map(str::trim)
        .filter(|term| !term.is_empty())
        .map(normalize_feature_name)
        .filter(|term| !term.is_empty())
        .collect()
}

fn normalize_feature_name(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

struct DetectedGpuInfo {
    count: Option<u32>,
    model: Option<String>,
}

fn detect_gpu_info() -> DetectedGpuInfo {
    let Some(output) = run_nvidia_smi_query() else {
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

fn run_nvidia_smi_query() -> Option<std::process::Output> {
    for candidate in nvidia_smi_candidates() {
        if candidate.contains('/') && !Path::new(candidate).exists() {
            continue;
        }
        if let Ok(output) = Command::new(candidate)
            .args(["--query-gpu=name", "--format=csv,noheader"])
            .output()
        {
            return Some(output);
        }
    }
    None
}

fn nvidia_smi_candidates() -> &'static [&'static str] {
    &[
        "nvidia-smi",
        "/usr/bin/nvidia-smi",
        "/usr/lib/wsl/lib/nvidia-smi",
        "/bin/nvidia-smi",
    ]
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

#[cfg(test)]
mod tests {
    use super::{nvidia_smi_candidates, parse_total_memory_mb};

    #[test]
    fn includes_common_nvidia_smi_locations() {
        let candidates = nvidia_smi_candidates();
        assert!(candidates.contains(&"nvidia-smi"));
        assert!(candidates.contains(&"/usr/lib/wsl/lib/nvidia-smi"));
    }

    #[test]
    fn parses_total_memory_from_proc_meminfo() {
        let meminfo = "MemTotal:       65843092 kB\nMemFree:         1024000 kB\n";
        assert_eq!(parse_total_memory_mb(meminfo), Some(64_299));
    }

    #[test]
    fn ignores_meminfo_without_memtotal_kb() {
        let meminfo = "MemFree:         1024000 kB\n";
        assert_eq!(parse_total_memory_mb(meminfo), None);
    }

    #[test]
    fn ignores_meminfo_with_unexpected_unit() {
        let meminfo = "MemTotal:       65843092 MB\n";
        assert_eq!(parse_total_memory_mb(meminfo), None);
    }
}
