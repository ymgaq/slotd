use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

use serde::{Deserialize, Serialize};

use crate::config::AppConfig;
use crate::error::{Result, SlotdError};
use crate::job::{JobRecord, NodeInfo, SubmitRequest};

#[derive(Debug, Serialize, Deserialize)]
pub enum Request {
    SubmitBatch(SubmitRequest),
    SubmitRun {
        request: SubmitRequest,
        immediate: bool,
    },
    ListJobs,
    Cancel {
        job_id: i64,
    },
    NodeInfo,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Response {
    Submitted { job_id: i64 },
    Jobs { jobs: Vec<JobRecord> },
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
