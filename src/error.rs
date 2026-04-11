use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SlotdError {
    #[error("process exited with code {0}")]
    Exit(i32),
    #[error("{0}")]
    Message(String),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("ipc error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("process error: {0}")]
    Nix(#[from] nix::Error),
}

pub type Result<T> = std::result::Result<T, SlotdError>;

impl From<&str> for SlotdError {
    fn from(value: &str) -> Self {
        Self::Message(value.to_string())
    }
}

impl From<String> for SlotdError {
    fn from(value: String) -> Self {
        Self::Message(value)
    }
}
