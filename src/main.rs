mod cgroup;
mod cli;
mod config;
mod cpu;
mod daemon;
mod env;
mod error;
mod ipc;
mod job;
mod launch;
mod notify;
mod output;
mod recovery;
mod runner;
mod sbatch;
mod signals;
mod store;
mod time;

use std::ffi::OsString;

use clap::Parser;

use crate::cli::{Cli, dispatch_argv0};
use crate::error::Result;

fn main() {
    if let Err(error) = run() {
        match error {
            crate::error::SlotdError::Exit(code) => std::process::exit(code),
            other => {
                eprintln!("error: {other}");
                std::process::exit(1);
            }
        }
    }
}

fn run() -> Result<()> {
    let argv: Vec<OsString> = std::env::args_os().collect();
    let argv = dispatch_argv0(argv);
    let cli = Cli::parse_from(argv);
    cli.run()
}
