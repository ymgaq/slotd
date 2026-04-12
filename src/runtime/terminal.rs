use crate::model::job::JobState;

pub(crate) fn exit_signal(status: &std::process::ExitStatus) -> Option<i32> {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status.signal()
    }
    #[cfg(not(unix))]
    {
        let _ = status;
        None
    }
}

pub(crate) fn terminal_state_with_reasons(
    exit_code: Option<i32>,
    term_signal: Option<i32>,
    oom_killed: bool,
    success_reason: &'static str,
    signal_reason: &'static str,
    exit_failure_reason: &'static str,
    unknown_reason: &'static str,
) -> (JobState, &'static str) {
    if oom_killed {
        return (JobState::OutOfMemory, "OutOfMemory");
    }
    match (exit_code, term_signal) {
        (Some(0), None) => (JobState::Completed, success_reason),
        (_, Some(_)) => (JobState::Failed, signal_reason),
        (Some(_), None) => (JobState::Failed, exit_failure_reason),
        (None, None) => (JobState::Failed, unknown_reason),
    }
}
