use crate::app::config::AppConfig;
use crate::app::error::{Result, SlotdError};
use crate::model::job::JobRecord;
use crate::proto::ipc::{Request, Response, send_request};
use crate::runtime::cgroup::{cgroup_oomed, cleanup_cgroup};
use crate::runtime::terminal::{exit_signal, terminal_state_with_reasons};

use super::{
    ForegroundExecutionOptions, launch_foreground_command, setup_local_cgroup, start_step_record,
};

pub(crate) fn run_foreground_step(
    config: &AppConfig,
    job: &JobRecord,
    command: &[String],
    options: ForegroundExecutionOptions<'_>,
) -> Result<()> {
    let step = start_step_record(config, job, command)?;
    let launched = launch_foreground_command(
        config,
        &step,
        command,
        ForegroundExecutionOptions {
            record_step: false,
            cpu_bind: options.cpu_bind.or(step.cpu_bind.as_deref()),
            io: options.io,
        },
    )?;
    let local_cgroup = match setup_local_cgroup(config, step.id, &step, launched.pid) {
        Ok(path) => path,
        Err(error) => {
            let mut child = launched.child;
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    match send_request(
        config,
        &Request::AdoptAllocation {
            job_id: step.id,
            pid: launched.pid,
            pgid: launched.pgid,
        },
    )? {
        Response::Submitted { .. } => {}
        Response::Error { message } => return Err(SlotdError::from(message)),
        other => {
            return Err(SlotdError::from(format!(
                "unexpected response while adopting step {}: {other:?}",
                step.id
            )));
        }
    }

    let mut child = launched.child;
    let status = child.wait()?;
    launched.io_state.finish()?;
    let exit_code = status.code();
    let term_signal = exit_signal(&status);
    let (state, reason) = terminal_state_with_reasons(
        exit_code,
        term_signal,
        cgroup_oomed(local_cgroup.as_deref()),
        "",
        "NonZeroExitCode",
        "NonZeroExitCode",
        "NonZeroExitCode",
    );
    match send_request(
        config,
        &Request::FinishAllocation {
            job_id: step.id,
            state,
            exit_code,
            term_signal,
            state_reason: Some(reason.to_string()),
        },
    )? {
        Response::Submitted { .. } => {}
        Response::Error { message } => return Err(SlotdError::from(message)),
        other => {
            return Err(SlotdError::from(format!(
                "unexpected response while finishing step {}: {other:?}",
                step.id
            )));
        }
    }
    cleanup_cgroup(local_cgroup.as_deref());
    match status.code() {
        Some(0) => Ok(()),
        Some(code) => Err(SlotdError::Exit(code)),
        None => Err(SlotdError::Exit(1)),
    }
}
