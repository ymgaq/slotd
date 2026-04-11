use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::config::AppConfig;
use crate::daemon;
use crate::error::{Result, SlotdError};
use crate::ipc::{Request, Response, send_request};
use crate::job::SubmitRequest;
use crate::output::{print_jobs, print_node_info};

#[derive(Debug, Parser)]
#[command(name = "slotd")]
#[command(about = "A single-node Slurm-like job scheduler")]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Daemon,
    Sbatch(SbatchArgs),
    Squeue,
    Scancel(ScancelArgs),
    Sinfo,
}

#[derive(Debug, Args)]
pub struct SbatchArgs {
    script: PathBuf,
    #[arg(long)]
    job_name: Option<String>,
    #[arg(long, default_value_t = 1)]
    cpus_per_task: u32,
    #[arg(long, default_value_t = 512)]
    mem: u64,
}

#[derive(Debug, Args)]
pub struct ScancelArgs {
    job_id: i64,
}

impl Cli {
    pub fn run(self) -> Result<()> {
        let config = AppConfig::load();
        match self.command {
            Commands::Daemon => daemon::run(config),
            Commands::Sbatch(args) => run_sbatch(config, args),
            Commands::Squeue => run_squeue(config),
            Commands::Scancel(args) => run_scancel(config, args),
            Commands::Sinfo => run_sinfo(config),
        }
    }
}

pub fn dispatch_argv0(mut argv: Vec<OsString>) -> Vec<OsString> {
    let Some(first) = argv.first() else {
        return argv;
    };

    let command = PathBuf::from(first)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("slotd")
        .to_string();

    let alias = match command.as_str() {
        "sbatch" => Some("sbatch"),
        "squeue" => Some("squeue"),
        "scancel" => Some("scancel"),
        "sinfo" => Some("sinfo"),
        _ => None,
    };

    if let Some(alias) = alias {
        argv.insert(1, OsString::from(alias));
    }
    argv
}

fn run_sbatch(config: AppConfig, args: SbatchArgs) -> Result<()> {
    let script_body = fs::read_to_string(&args.script)?;
    let cwd = std::env::current_dir()?.to_string_lossy().to_string();
    let script_name = args
        .script
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("script.sh")
        .to_string();

    let request = SubmitRequest {
        name: args.job_name,
        cwd,
        script_name,
        script_body,
        requested_cpus: args.cpus_per_task,
        requested_memory_mb: args.mem,
    };

    match send_request(&config, &Request::SubmitBatch(request))? {
        Response::Submitted { job_id } => {
            println!("Submitted batch job {job_id}");
            Ok(())
        }
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response to sbatch: {other:?}"
        ))),
    }
}

fn run_squeue(config: AppConfig) -> Result<()> {
    match send_request(&config, &Request::ListJobs)? {
        Response::Jobs { jobs } => {
            print_jobs(&jobs);
            Ok(())
        }
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response to squeue: {other:?}"
        ))),
    }
}

fn run_scancel(config: AppConfig, args: ScancelArgs) -> Result<()> {
    match send_request(
        &config,
        &Request::Cancel {
            job_id: args.job_id,
        },
    )? {
        Response::Cancelled { job_id } => {
            println!("Cancelled job {job_id}");
            Ok(())
        }
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response to scancel: {other:?}"
        ))),
    }
}

fn run_sinfo(config: AppConfig) -> Result<()> {
    match send_request(&config, &Request::NodeInfo)? {
        Response::NodeInfo { info } => {
            print_node_info(&info);
            Ok(())
        }
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response to sinfo: {other:?}"
        ))),
    }
}
