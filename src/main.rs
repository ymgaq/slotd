mod app;
mod command;
mod format;
mod model;
mod proto;
mod runtime;
mod store;
mod submit;
mod util;

use std::ffi::OsString;

use clap::Parser;

use crate::app::error::Result;
use crate::command::cli::{Cli, dispatch_argv0};

fn main() {
    if let Err(error) = run() {
        match error {
            crate::app::error::SlotdError::Exit(code) => std::process::exit(code),
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
