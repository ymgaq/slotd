use crate::error::{Result, SlotdError};
use crate::job::WarningSignal;

pub(crate) fn parse_warning_signal(value: &str) -> Result<WarningSignal> {
    let trimmed = value.trim();
    let trimmed = trimmed.strip_prefix("B:").unwrap_or(trimmed);
    let (signal_name, seconds_before_end) = match trimmed.split_once('@') {
        Some((signal_name, seconds_before_end)) => (
            signal_name,
            seconds_before_end
                .parse::<u64>()
                .map_err(|_| SlotdError::from(format!("invalid signal offset: {trimmed}")))?,
        ),
        None => (trimmed, 60),
    };
    Ok(WarningSignal {
        signal: parse_signal_name(signal_name)?,
        seconds_before_end,
    })
}

pub(crate) fn parse_signal_name(value: &str) -> Result<i32> {
    let normalized = value.trim().trim_start_matches("SIG").to_ascii_uppercase();
    let signal = match normalized.as_str() {
        "TERM" => nix::sys::signal::Signal::SIGTERM,
        "KILL" => nix::sys::signal::Signal::SIGKILL,
        "INT" => nix::sys::signal::Signal::SIGINT,
        "HUP" => nix::sys::signal::Signal::SIGHUP,
        "QUIT" => nix::sys::signal::Signal::SIGQUIT,
        "USR1" => nix::sys::signal::Signal::SIGUSR1,
        "USR2" => nix::sys::signal::Signal::SIGUSR2,
        "CONT" => nix::sys::signal::Signal::SIGCONT,
        "STOP" => nix::sys::signal::Signal::SIGSTOP,
        "TSTP" => nix::sys::signal::Signal::SIGTSTP,
        "ALRM" => nix::sys::signal::Signal::SIGALRM,
        other => {
            if let Ok(number) = other.parse::<i32>() {
                return Ok(number);
            }
            return Err(SlotdError::from(format!("unsupported signal: {value}")));
        }
    };
    Ok(signal as i32)
}
