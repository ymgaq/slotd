use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::{FromRawFd, IntoRawFd, OwnedFd};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::app::error::{Result, SlotdError};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ForegroundIoOptions<'a> {
    pub(crate) stdout_path: Option<&'a Path>,
    pub(crate) stderr_path: Option<&'a Path>,
    pub(crate) label_output: bool,
    pub(crate) unbuffered: bool,
}

pub(crate) enum ForegroundIoState {
    Direct,
    Streamed(Vec<std::thread::JoinHandle<Result<()>>>),
}

impl ForegroundIoState {
    pub(crate) fn finish(self) -> Result<()> {
        match self {
            Self::Direct => Ok(()),
            Self::Streamed(handles) => {
                for handle in handles {
                    handle
                        .join()
                        .map_err(|_| SlotdError::from("foreground output thread panicked"))??;
                }
                Ok(())
            }
        }
    }
}

pub(crate) fn configure_foreground_stdio(
    command: &mut Command,
    cwd: &str,
    stdout_path: Option<&Path>,
    stderr_path: Option<&Path>,
    label_output: bool,
    unbuffered: bool,
) -> Result<ForegroundIoState> {
    if !label_output && !unbuffered {
        apply_foreground_stdio(command, cwd, stdout_path, stderr_path)?;
        return Ok(ForegroundIoState::Direct);
    }

    let stdout_target = open_foreground_stdio(stdout_path, cwd, "/dev/stdout")?;
    let stderr_target = if same_path(stdout_path, stderr_path) {
        stdout_target.try_clone()?
    } else {
        open_foreground_stdio(stderr_path, cwd, "/dev/stderr")?
    };

    let (stdout_reader, stdout_writer) = std::os::unix::net::UnixStream::pair()?;
    let (stderr_reader, stderr_writer) = std::os::unix::net::UnixStream::pair()?;
    let stdout_writer = unsafe { OwnedFd::from_raw_fd(stdout_writer.into_raw_fd()) };
    let stderr_writer = unsafe { OwnedFd::from_raw_fd(stderr_writer.into_raw_fd()) };
    command.stdout(Stdio::from(stdout_writer));
    command.stderr(Stdio::from(stderr_writer));

    let stdout_handle = spawn_output_forwarder(
        stdout_reader,
        stdout_target,
        label_output,
        unbuffered,
        "0: ",
    )?;
    let stderr_handle = spawn_output_forwarder(
        stderr_reader,
        stderr_target,
        label_output,
        unbuffered,
        "0: ",
    )?;
    Ok(ForegroundIoState::Streamed(vec![
        stdout_handle,
        stderr_handle,
    ]))
}

fn apply_foreground_stdio(
    command: &mut Command,
    cwd: &str,
    stdout_path: Option<&Path>,
    stderr_path: Option<&Path>,
) -> Result<()> {
    let stdout = open_foreground_stdio(stdout_path, cwd, "/dev/stdout")?;
    let stderr = if same_path(stdout_path, stderr_path) {
        stdout.try_clone()?
    } else {
        open_foreground_stdio(stderr_path, cwd, "/dev/stderr")?
    };
    command.stdout(Stdio::from(stdout));
    command.stderr(Stdio::from(stderr));
    Ok(())
}

fn open_foreground_stdio(path: Option<&Path>, cwd: &str, fallback: &str) -> Result<std::fs::File> {
    let Some(path) = path else {
        return open_stdio_handle(fallback);
    };
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        Path::new(cwd).join(path)
    };
    let mut options = OpenOptions::new();
    options.create(true).write(true).truncate(true);
    Ok(options.open(resolved)?)
}

fn same_path(stdout_path: Option<&Path>, stderr_path: Option<&Path>) -> bool {
    match (stdout_path, stderr_path) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

fn open_stdio_handle(path: &str) -> Result<std::fs::File> {
    Ok(OpenOptions::new().write(true).open(path)?)
}

fn spawn_output_forwarder(
    reader: std::os::unix::net::UnixStream,
    mut target: std::fs::File,
    label_output: bool,
    unbuffered: bool,
    label_prefix: &'static str,
) -> Result<std::thread::JoinHandle<Result<()>>> {
    Ok(std::thread::spawn(move || {
        if label_output {
            let mut reader = BufReader::new(reader);
            let mut line = Vec::new();
            loop {
                line.clear();
                let bytes = reader.read_until(b'\n', &mut line)?;
                if bytes == 0 {
                    break;
                }
                target.write_all(label_prefix.as_bytes())?;
                target.write_all(&line)?;
                target.flush()?;
            }
            return Ok(());
        }

        let mut reader = reader;
        let mut buf = [0u8; 4096];
        loop {
            let bytes = reader.read(&mut buf)?;
            if bytes == 0 {
                break;
            }
            target.write_all(&buf[..bytes])?;
            if unbuffered {
                target.flush()?;
            }
        }
        if !unbuffered {
            target.flush()?;
        }
        Ok(())
    }))
}
