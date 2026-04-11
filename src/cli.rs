use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::config::AppConfig;
use crate::daemon;
use crate::error::{Result, SlotdError};
use crate::ipc::{Request, Response, send_request};
use crate::job::{JobState, SubmitRequest};
use crate::output::{print_sacct_jobs, print_sinfo, print_squeue_jobs};
use crate::sbatch::{parse_directives, parse_mem_mb};

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
    Srun(SrunArgs),
    Squeue(SqueueArgs),
    Sacct(SacctArgs),
    Scancel(ScancelArgs),
    Sinfo,
}

#[derive(Debug, Args)]
pub struct SbatchArgs {
    script: PathBuf,
    #[arg(long)]
    job_name: Option<String>,
    #[arg(long, short = 'p')]
    partition: Option<String>,
    #[arg(long)]
    cpus_per_task: Option<u32>,
    #[arg(long)]
    mem: Option<String>,
    #[arg(long)]
    gpus: Option<u32>,
    #[arg(long)]
    output: Option<PathBuf>,
    #[arg(long)]
    error: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct ScancelArgs {
    job_id: i64,
}

#[derive(Debug, Args)]
pub struct SqueueArgs {
    #[arg(long)]
    all: bool,
    #[arg(long, value_delimiter = ',')]
    states: Option<Vec<String>>,
}

#[derive(Debug, Args)]
pub struct SacctArgs {
    #[arg(short = 'j', long = "jobs", value_delimiter = ',')]
    jobs: Option<Vec<i64>>,
    #[arg(short = 's', long = "state", value_delimiter = ',')]
    states: Option<Vec<String>>,
}

#[derive(Debug, Args)]
pub struct SrunArgs {
    #[arg(long)]
    job_name: Option<String>,
    #[arg(long, short = 'p')]
    partition: Option<String>,
    #[arg(long)]
    cpus_per_task: Option<u32>,
    #[arg(long)]
    mem: Option<String>,
    #[arg(long)]
    gpus: Option<u32>,
    #[arg(long)]
    output: Option<PathBuf>,
    #[arg(long)]
    error: Option<PathBuf>,
    #[arg(long)]
    immediate: bool,
    #[arg(required = true, num_args = 1.., trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<String>,
}

impl Cli {
    pub fn run(self) -> Result<()> {
        let config = AppConfig::load();
        match self.command {
            Commands::Daemon => daemon::run(config),
            Commands::Sbatch(args) => run_sbatch(config, args),
            Commands::Srun(args) => run_srun(config, args),
            Commands::Squeue(args) => run_squeue(config, args),
            Commands::Sacct(args) => run_sacct(config, args),
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
        "srun" => Some("srun"),
        "squeue" => Some("squeue"),
        "sacct" => Some("sacct"),
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
    let directives = parse_directives(&script_body)?;
    let cwd = std::env::current_dir()?.to_string_lossy().to_string();
    let script_name = args
        .script
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("script.sh")
        .to_string();
    let partition = args
        .partition
        .or(directives.partition)
        .unwrap_or_else(|| config.default_partition().to_string());
    if !config.has_partition(&partition) {
        return Err(SlotdError::from(format!("unknown partition: {partition}")));
    }
    let requested_cpus = args.cpus_per_task.or(directives.cpus_per_task).unwrap_or(1);
    let requested_memory_mb = match args.mem {
        Some(value) => parse_mem_mb(&value)?,
        None => directives.mem_mb.unwrap_or(512),
    };
    let requested_gpus = args
        .gpus
        .or(directives.gpus)
        .unwrap_or_else(|| config.default_gpus_for_partition(&partition));

    let request = SubmitRequest {
        name: args.job_name.or(directives.job_name),
        user_name: current_user_name(),
        partition,
        cwd,
        script_name,
        script_body,
        command_override: None,
        requested_cpus,
        requested_memory_mb,
        requested_gpus,
        stdout_path: args
            .output
            .map(|path| path.to_string_lossy().to_string())
            .or(directives.output_path),
        stderr_path: args
            .error
            .map(|path| path.to_string_lossy().to_string())
            .or(directives.error_path),
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

fn run_srun(config: AppConfig, args: SrunArgs) -> Result<()> {
    let cwd = std::env::current_dir()?.to_string_lossy().to_string();
    let command_name = args
        .command
        .first()
        .map(|value| command_basename(value))
        .unwrap_or_else(|| "srun".to_string());
    let partition = args
        .partition
        .unwrap_or_else(|| config.default_partition().to_string());
    if !config.has_partition(&partition) {
        return Err(SlotdError::from(format!("unknown partition: {partition}")));
    }
    let requested_cpus = args.cpus_per_task.unwrap_or(1);
    let requested_memory_mb = match args.mem {
        Some(value) => parse_mem_mb(&value)?,
        None => 512,
    };
    let requested_gpus = args
        .gpus
        .unwrap_or_else(|| config.default_gpus_for_partition(&partition));
    let command_override = shell_join(&args.command);
    let script_body = format!("#!/usr/bin/env bash\nexec {}\n", command_override);

    let request = SubmitRequest {
        name: args.job_name.or_else(|| Some(command_name.clone())),
        user_name: current_user_name(),
        partition,
        cwd,
        script_name: command_name,
        script_body,
        command_override: Some(command_override),
        requested_cpus,
        requested_memory_mb,
        requested_gpus,
        stdout_path: args.output.map(|path| path.to_string_lossy().to_string()),
        stderr_path: args.error.map(|path| path.to_string_lossy().to_string()),
    };

    match send_request(
        &config,
        &Request::SubmitRun {
            request,
            immediate: args.immediate,
        },
    )? {
        Response::Submitted { job_id } => {
            println!("Submitted run job {job_id}");
            Ok(())
        }
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response to srun: {other:?}"
        ))),
    }
}

fn run_squeue(config: AppConfig, args: SqueueArgs) -> Result<()> {
    let state_filter = if let Some(values) = args.states {
        Some(parse_states(values)?)
    } else if args.all {
        None
    } else {
        Some(vec![JobState::Pending, JobState::Running])
    };

    match send_request(&config, &Request::ListJobs { states: None })? {
        Response::Jobs { jobs } => {
            let jobs = filter_jobs(jobs, state_filter.as_deref(), None);
            print_squeue_jobs(&config, &jobs);
            Ok(())
        }
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response to squeue: {other:?}"
        ))),
    }
}

fn run_sacct(config: AppConfig, args: SacctArgs) -> Result<()> {
    let state_filter = args.states.map(parse_states).transpose()?;
    match send_request(
        &config,
        &Request::ListAccountingJobs {
            states: None,
            ids: args.jobs,
        },
    )? {
        Response::Jobs { jobs } => {
            let jobs = filter_jobs(jobs, state_filter.as_deref(), None);
            print_sacct_jobs(&jobs);
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
            print_sinfo(&info);
            Ok(())
        }
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response to sinfo: {other:?}"
        ))),
    }
}

fn shell_join(args: &[String]) -> String {
    args.iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }

    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '.' | ':'))
    {
        return value.to_string();
    }

    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn command_basename(command: &str) -> String {
    PathBuf::from(command)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("srun")
        .to_string()
}

fn current_user_name() -> String {
    std::env::var("USER").unwrap_or_else(|_| "unknown".to_string())
}

fn parse_states(values: Vec<String>) -> Result<Vec<JobState>> {
    values
        .into_iter()
        .map(|value| parse_state(&value))
        .collect()
}

fn parse_state(value: &str) -> Result<JobState> {
    let normalized = value.trim().to_ascii_uppercase();
    match normalized.as_str() {
        "PD" | "PENDING" => Ok(JobState::Pending),
        "R" | "RUNNING" => Ok(JobState::Running),
        "CD" | "COMPLETED" => Ok(JobState::Completed),
        "F" | "FAILED" => Ok(JobState::Failed),
        "CA" | "CANCELLED" => Ok(JobState::Cancelled),
        _ => Err(SlotdError::from(format!("unknown state: {value}"))),
    }
}

fn filter_jobs(
    jobs: Vec<crate::job::JobRecord>,
    states: Option<&[JobState]>,
    ids: Option<&[i64]>,
) -> Vec<crate::job::JobRecord> {
    jobs.into_iter()
        .filter(|job| {
            let state_ok = states
                .map(|states| states.contains(&job.state))
                .unwrap_or(true);
            let id_ok = ids.map(|ids| ids.contains(&job.id)).unwrap_or(true);
            state_ok && id_ok
        })
        .collect()
}
