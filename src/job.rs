use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobState {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl JobState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Running => "RUNNING",
            Self::Completed => "COMPLETED",
            Self::Failed => "FAILED",
            Self::Cancelled => "CANCELLED",
        }
    }

    pub fn short_code(self) -> &'static str {
        match self {
            Self::Pending => "PD",
            Self::Running => "R",
            Self::Completed => "CD",
            Self::Failed => "F",
            Self::Cancelled => "CA",
        }
    }
}

impl std::str::FromStr for JobState {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "PENDING" => Ok(Self::Pending),
            "RUNNING" => Ok(Self::Running),
            "COMPLETED" => Ok(Self::Completed),
            "FAILED" => Ok(Self::Failed),
            "CANCELLED" => Ok(Self::Cancelled),
            other => Err(format!("unknown job state: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRecord {
    pub id: i64,
    pub name: String,
    pub state: JobState,
    pub command: String,
    pub cwd: String,
    pub requested_cpus: u32,
    pub requested_memory_mb: u64,
    pub submit_time: i64,
    pub start_time: Option<i64>,
    pub end_time: Option<i64>,
    pub pid: Option<i32>,
    pub pgid: Option<i32>,
    pub exit_code: Option<i32>,
    pub script_path: String,
    pub stdout_path: String,
    pub stderr_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitRequest {
    pub name: Option<String>,
    pub cwd: String,
    pub script_name: String,
    pub script_body: String,
    pub requested_cpus: u32,
    pub requested_memory_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub total_cpus: u32,
    pub total_memory_mb: u64,
    pub allocated_cpus: u32,
    pub allocated_memory_mb: u64,
    pub running_jobs: usize,
    pub pending_jobs: usize,
}
