mod allocation;
mod ipc;
mod step;

use crate::runtime::foreground_io::ForegroundIoOptions;

pub(crate) use allocation::{run_foreground_allocation, run_foreground_allocation_with_mode};
pub(crate) use step::run_foreground_step;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ForegroundExecutionOptions<'a> {
    pub(crate) record_step: bool,
    pub(crate) cpu_bind: Option<&'a str>,
    pub(crate) io: ForegroundIoOptions<'a>,
}
