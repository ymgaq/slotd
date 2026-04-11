use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobState {
    Pending,
    Running,
    Completing,
    Completed,
    Failed,
    Cancelled,
    Timeout,
    OutOfMemory,
}

impl JobState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Running => "RUNNING",
            Self::Completing => "COMPLETING",
            Self::Completed => "COMPLETED",
            Self::Failed => "FAILED",
            Self::Cancelled => "CANCELLED",
            Self::Timeout => "TIMEOUT",
            Self::OutOfMemory => "OUT_OF_MEMORY",
        }
    }

    pub fn short_code(self) -> &'static str {
        match self {
            Self::Pending => "PD",
            Self::Running => "R",
            Self::Completing => "CG",
            Self::Completed => "CD",
            Self::Failed => "F",
            Self::Cancelled => "CA",
            Self::Timeout => "TO",
            Self::OutOfMemory => "OOM",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::Timeout | Self::OutOfMemory
        )
    }
}

impl std::str::FromStr for JobState {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "PENDING" => Ok(Self::Pending),
            "RUNNING" => Ok(Self::Running),
            "COMPLETING" => Ok(Self::Completing),
            "COMPLETED" => Ok(Self::Completed),
            "FAILED" => Ok(Self::Failed),
            "CANCELLED" => Ok(Self::Cancelled),
            "TIMEOUT" => Ok(Self::Timeout),
            "OUT_OF_MEMORY" => Ok(Self::OutOfMemory),
            other => Err(format!("unknown job state: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRecord {
    pub id: i64,
    pub name: String,
    pub user_name: String,
    pub state: JobState,
    pub partition: String,
    pub command: String,
    pub cwd: String,
    pub requested_cpus: u32,
    pub requested_tasks: u32,
    pub requested_memory_mb: u64,
    pub requested_gpus: u32,
    pub allocation_only: bool,
    pub submit_time: i64,
    pub start_time: Option<i64>,
    pub end_time: Option<i64>,
    pub pid: Option<i32>,
    pub pgid: Option<i32>,
    pub exit_code: Option<i32>,
    pub state_reason: Option<String>,
    pub term_signal: Option<i32>,
    pub time_limit_secs: Option<u64>,
    pub assigned_gpu_ids: Vec<u32>,
    pub script_path: String,
    pub stdout_path: String,
    pub stderr_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitRequest {
    pub name: Option<String>,
    pub user_name: String,
    pub partition: String,
    pub cwd: String,
    pub script_name: String,
    pub script_body: String,
    pub command_override: Option<String>,
    pub requested_cpus: u32,
    pub requested_tasks: u32,
    pub requested_memory_mb: u64,
    pub requested_gpus: u32,
    pub allocation_only: bool,
    pub time_limit_secs: Option<u64>,
    pub stdout_path: Option<String>,
    pub stderr_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub partitions: Vec<PartitionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionInfo {
    pub name: String,
    pub hostname: String,
    pub state: String,
    pub gres_used: String,
    pub total_cpus: u32,
    pub total_memory_mb: u64,
    pub total_gpus: u32,
    pub allocated_cpus: u32,
    pub allocated_memory_mb: u64,
    pub allocated_gpus: u32,
    pub running_jobs: usize,
    pub pending_jobs: usize,
}
