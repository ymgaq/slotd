use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

use serde::{Deserialize, Serialize};

use crate::config::AppConfig;
use crate::error::{Result, SlotdError};
use crate::job::{JobRecord, JobState, NodeInfo, SubmitRequest};

#[derive(Debug, Serialize, Deserialize)]
pub enum Request {
    SubmitBatch(SubmitRequest),
    SubmitRun {
        request: SubmitRequest,
        immediate: bool,
    },
    SubmitAlloc {
        request: SubmitRequest,
        immediate: bool,
    },
    StartStep {
        parent_job_id: i64,
        name: String,
        command: String,
        cwd: String,
        user_name: String,
    },
    AdoptAllocation {
        job_id: i64,
        pid: i32,
        pgid: i32,
    },
    FinishAllocation {
        job_id: i64,
        state: JobState,
        exit_code: Option<i32>,
        term_signal: Option<i32>,
        state_reason: Option<String>,
    },
    ListJobs {
        states: Option<Vec<JobState>>,
        ids: Option<Vec<i64>>,
        user_name: Option<String>,
        partitions: Option<Vec<String>>,
    },
    ListAccountingJobs {
        states: Option<Vec<JobState>>,
        ids: Option<Vec<i64>>,
        user_name: Option<String>,
        partitions: Option<Vec<String>>,
        start_time: Option<i64>,
        end_time: Option<i64>,
    },
    ListSteps {
        parent_job_id: i64,
    },
    GetJob {
        job_id: i64,
    },
    HoldJob {
        job_id: i64,
    },
    ReleaseJob {
        job_id: i64,
    },
    UpdateJob {
        job_id: i64,
        name: Option<String>,
        partition: Option<String>,
        time_limit_secs: Option<u64>,
        priority: Option<i32>,
    },
    Cancel {
        job_id: i64,
    },
    NodeInfo,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Response {
    Submitted { job_id: i64 },
    Jobs { jobs: Vec<JobRecord> },
    Job { job: Option<JobRecord> },
    Cancelled { job_id: i64 },
    NodeInfo { info: NodeInfo },
    Error { message: String },
}

pub fn send_request(config: &AppConfig, request: &Request) -> Result<Response> {
    let mut stream = UnixStream::connect(&config.socket_path)?;
    let body = serde_json::to_vec(request)?;
    stream.write_all(&body)?;
    stream.write_all(b"\n")?;
    stream.flush()?;

    let mut response = String::new();
    let mut reader = BufReader::new(stream);
    reader.read_line(&mut response)?;
    if response.is_empty() {
        return Err(SlotdError::from(
            "daemon closed connection without a response",
        ));
    }

    let response: Response = serde_json::from_str(&response)?;
    Ok(response)
}
