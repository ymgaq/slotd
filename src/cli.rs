use std::ffi::OsString;
use std::fs;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::{FromRawFd, IntoRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use clap::{Args, Parser, Subcommand};

use crate::config::AppConfig;
use crate::daemon;
use crate::error::{Result, SlotdError};
use crate::ipc::{Request, Response, send_request};
use crate::job::{JobRecord, JobState, OpenMode, SubmitRequest, WarningSignal};
use crate::launch::{LaunchCommand, build_multitask_launcher, shell_join};
use crate::output::{
    parse_sacct_fields, parse_sinfo_fields, parse_squeue_fields, print_sacct_jobs,
    print_sacct_jobs_delimited, print_sinfo, print_squeue_jobs, print_squeue_jobs_with_options,
    print_squeue_jobs_with_start_times,
};
use crate::sbatch::{BatchDirectives, parse_directives, parse_mem_mb, parse_time_limit_secs};

#[cfg(test)]
const SUPPORTED_ROOT_COMMANDS: &[&str] = &[
    "daemon", "sbatch", "srun", "salloc", "scontrol", "squeue", "sacct", "scancel", "sinfo",
];
#[cfg(test)]
const SUPPORTED_USER_COMMANDS: &[&str] = &[
    "sbatch", "srun", "salloc", "scontrol", "squeue", "sacct", "scancel", "sinfo",
];
#[cfg(test)]
const CORE_RESOURCE_LONG_FLAGS: &[&str] = &[
    "job-name",
    "partition",
    "cpus-per-task",
    "ntasks",
    "mem",
    "time",
    "gpus",
    "chdir",
    "constraint",
];

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
    Salloc(SallocArgs),
    Scontrol(ScontrolArgs),
    Squeue(SqueueArgs),
    Sacct(SacctArgs),
    Scancel(ScancelArgs),
    Sinfo(SinfoArgs),
}

#[derive(Debug, Args)]
pub struct ResourceArgs {
    #[arg(long, short = 'J')]
    job_name: Option<String>,
    #[arg(long, short = 'p')]
    partition: Option<String>,
    #[arg(long, short = 'c')]
    cpus_per_task: Option<u32>,
    #[arg(long, short = 'n')]
    ntasks: Option<u32>,
    #[arg(long)]
    mem: Option<String>,
    #[arg(long, short = 't')]
    time: Option<String>,
    #[arg(long, short = 'G')]
    gpus: Option<u32>,
    #[arg(long, short = 'D')]
    chdir: Option<PathBuf>,
    #[arg(long)]
    constraint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedResourceArgs {
    job_name: Option<String>,
    partition: String,
    cwd: String,
    requested_cpus: u32,
    requested_tasks: u32,
    requested_memory_mb: u64,
    requested_gpus: u32,
    time_limit_secs: Option<u64>,
    constraint: Option<String>,
}

#[derive(Debug, Default, Clone)]
struct SbatchEnvOverrides {
    directives: BatchDirectives,
    export: Option<String>,
    export_file: Option<PathBuf>,
    open_mode: Option<String>,
    signal: Option<String>,
    begin: Option<String>,
    exclusive: bool,
    requeue: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct ForegroundIoOptions<'a> {
    stdout_path: Option<&'a Path>,
    stderr_path: Option<&'a Path>,
    label_output: bool,
    unbuffered: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct ForegroundExecutionOptions<'a> {
    record_step: bool,
    cpu_bind: Option<&'a str>,
    io: ForegroundIoOptions<'a>,
}

impl ResourceArgs {
    fn resolve(
        &self,
        config: &AppConfig,
        directives: Option<&BatchDirectives>,
    ) -> Result<ResolvedResourceArgs> {
        let cwd = self
            .chdir
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
            .or_else(|| directives.and_then(|value| value.chdir.clone()))
            .unwrap_or_else(current_dir_string);
        let partition = self
            .partition
            .clone()
            .or_else(|| directives.and_then(|value| value.partition.clone()))
            .unwrap_or_else(|| config.default_partition().to_string());
        if !config.has_partition(&partition) {
            return Err(SlotdError::from(format!("unknown partition: {partition}")));
        }

        let requested_cpus = self
            .cpus_per_task
            .or_else(|| directives.and_then(|value| value.cpus_per_task))
            .unwrap_or(1);
        let requested_tasks = self
            .ntasks
            .or_else(|| directives.and_then(|value| value.ntasks))
            .unwrap_or(1);
        let requested_memory_mb = match &self.mem {
            Some(value) => parse_mem_mb(value)?,
            None => directives.and_then(|value| value.mem_mb).unwrap_or(512),
        };
        let requested_gpus = self
            .gpus
            .or_else(|| directives.and_then(|value| value.gpus))
            .unwrap_or_else(|| config.default_gpus_for_partition(&partition));
        let constraint = self
            .constraint
            .clone()
            .or_else(|| directives.and_then(|value| value.constraint.clone()));
        if let Some(value) = constraint.as_deref() {
            validate_constraint(config, value, &partition)?;
        }
        let time_limit_secs = match &self.time {
            Some(value) => Some(parse_time_limit_secs(value)?),
            None => directives.and_then(|value| value.time_limit_secs),
        };

        Ok(ResolvedResourceArgs {
            job_name: self
                .job_name
                .clone()
                .or_else(|| directives.and_then(|value| value.job_name.clone())),
            partition,
            cwd,
            requested_cpus,
            requested_tasks,
            requested_memory_mb,
            requested_gpus,
            time_limit_secs,
            constraint,
        })
    }
}

#[derive(Debug, Args)]
pub struct SbatchArgs {
    #[arg(required_unless_present = "wrap", conflicts_with = "wrap")]
    script: Option<PathBuf>,
    #[arg(long)]
    wrap: Option<String>,
    #[command(flatten)]
    resources: ResourceArgs,
    #[arg(long, short = 'o')]
    output: Option<PathBuf>,
    #[arg(long, short = 'e')]
    error: Option<PathBuf>,
    #[arg(long)]
    export: Option<String>,
    #[arg(long = "export-file")]
    export_file: Option<PathBuf>,
    #[arg(long = "open-mode")]
    open_mode: Option<String>,
    #[arg(long)]
    signal: Option<String>,
    #[arg(long)]
    begin: Option<String>,
    #[arg(long)]
    exclusive: bool,
    #[arg(long)]
    requeue: bool,
    #[arg(long, short = 'd')]
    dependency: Option<String>,
    #[arg(long, short = 'a')]
    array: Option<String>,
    #[arg(long)]
    parsable: bool,
    #[arg(long, short = 'W')]
    wait: bool,
}

#[derive(Debug, Args)]
pub struct ScontrolArgs {
    action: String,
    entity: String,
    job_id: i64,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    updates: Vec<String>,
}

#[derive(Debug, Args)]
pub struct ScancelArgs {
    #[arg(long, short = 's')]
    signal: Option<String>,
    job_id: String,
}

#[derive(Debug, Args)]
pub struct SqueueArgs {
    #[arg(long)]
    all: bool,
    #[arg(short = 't', long, value_delimiter = ',')]
    states: Option<Vec<String>>,
    #[arg(short = 'j', long = "jobs", value_delimiter = ',')]
    jobs: Option<Vec<i64>>,
    #[arg(short = 'u', long = "user")]
    user: Option<String>,
    #[arg(short = 'p', long = "partition", value_delimiter = ',')]
    partitions: Option<Vec<String>>,
    #[arg(short = 'o', long = "format")]
    format: Option<String>,
    #[arg(short = 'S', long = "sort")]
    sort: Option<String>,
    #[arg(short = 'l', long = "long")]
    long: bool,
    #[arg(long)]
    start: bool,
    #[arg(long)]
    array: bool,
    #[arg(long = "noheader")]
    noheader: bool,
}

#[derive(Debug, Args)]
pub struct SacctArgs {
    #[arg(short = 'j', long = "jobs", value_delimiter = ',')]
    jobs: Option<Vec<i64>>,
    #[arg(short = 's', long = "state", value_delimiter = ',')]
    states: Option<Vec<String>>,
    #[arg(short = 'S', long = "starttime")]
    start_time: Option<String>,
    #[arg(short = 'E', long = "endtime")]
    end_time: Option<String>,
    #[arg(short = 'u', long = "user")]
    user: Option<String>,
    #[arg(short = 'p', long = "partition", value_delimiter = ',')]
    partitions: Option<Vec<String>>,
    #[arg(short = 'o', long = "format")]
    format: Option<String>,
    #[arg(short = 'P', long = "parsable2")]
    parsable2: bool,
    #[arg(short = 'n', long = "noheader")]
    noheader: bool,
}

#[derive(Debug, Args)]
pub struct SrunArgs {
    #[command(flatten)]
    resources: ResourceArgs,
    #[arg(long, short = 'o')]
    output: Option<PathBuf>,
    #[arg(long, short = 'e')]
    error: Option<PathBuf>,
    #[arg(long)]
    immediate: bool,
    #[arg(long)]
    pty: bool,
    #[arg(long = "cpu-bind")]
    cpu_bind: Option<String>,
    #[arg(long)]
    label: bool,
    #[arg(long)]
    unbuffered: bool,
    #[arg(long, hide = true)]
    no_wait: bool,
    #[arg(required = true, num_args = 1.., trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<String>,
}

#[derive(Debug, Args)]
pub struct SallocArgs {
    #[command(flatten)]
    resources: ResourceArgs,
    #[arg(long)]
    immediate: bool,
    #[arg(num_args = 0.., trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<String>,
}

#[derive(Debug, Args)]
pub struct SinfoArgs {
    #[arg(short = 'p', long = "partition", value_delimiter = ',')]
    partitions: Option<Vec<String>>,
    #[arg(short = 'N', long = "Node")]
    node: bool,
    #[arg(short = 'l', long = "long")]
    long: bool,
    #[arg(short = 'o', long = "format")]
    format: Option<String>,
    #[arg(long = "noheader")]
    noheader: bool,
}

impl Cli {
    pub fn run(self) -> Result<()> {
        let config = AppConfig::load();
        match self.command {
            Commands::Daemon => daemon::run(config),
            Commands::Sbatch(args) => run_sbatch(config, args),
            Commands::Srun(args) => run_srun(config, args),
            Commands::Salloc(args) => run_salloc(config, args),
            Commands::Scontrol(args) => run_scontrol(config, args),
            Commands::Squeue(args) => run_squeue(config, args),
            Commands::Sacct(args) => run_sacct(config, args),
            Commands::Scancel(args) => run_scancel(config, args),
            Commands::Sinfo(args) => run_sinfo(config, args),
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
        "salloc" => Some("salloc"),
        "scontrol" => Some("scontrol"),
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
    let (script_name, script_body, directives, command_override) = if let Some(wrap) = &args.wrap {
        (
            "wrap".to_string(),
            format!("#!/usr/bin/env bash\n{}\n", wrap),
            crate::sbatch::BatchDirectives::default(),
            Some(wrap.clone()),
        )
    } else {
        let script = args
            .script
            .as_ref()
            .ok_or_else(|| SlotdError::from("script is required"))?;
        let script_body = fs::read_to_string(script)?;
        let directives = parse_directives(&script_body)?;
        let script_name = script
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("script.sh")
            .to_string();
        (script_name, script_body, directives, None)
    };
    let env_overrides = load_sbatch_env_overrides();
    let defaults = merge_batch_directives(&directives, &env_overrides.directives);
    let resolved = args.resources.resolve(&config, Some(&defaults))?;
    let export_env = resolve_export_env(
        args.export.as_deref().or(env_overrides.export.as_deref()),
        args.export_file
            .as_deref()
            .or(env_overrides.export_file.as_deref()),
    )?;
    let open_mode = args
        .open_mode
        .as_deref()
        .or(env_overrides.open_mode.as_deref())
        .unwrap_or("truncate")
        .parse::<OpenMode>()
        .map_err(SlotdError::from)?;
    let warning_signal = args
        .signal
        .as_deref()
        .or(env_overrides.signal.as_deref())
        .map(parse_warning_signal)
        .transpose()?;
    let begin_time = args
        .begin
        .as_deref()
        .or(env_overrides.begin.as_deref())
        .or(defaults.begin.as_deref())
        .map(parse_begin_time)
        .transpose()?;
    let exclusive = args.exclusive || env_overrides.exclusive || defaults.exclusive;
    let requeue = args.requeue || env_overrides.requeue || defaults.requeue;

    let request = SubmitRequest {
        name: resolved.job_name,
        user_name: current_user_name(),
        partition: resolved.partition,
        cwd: resolved.cwd,
        script_name,
        script_body,
        command_override,
        requested_cpus: resolved.requested_cpus,
        requested_tasks: resolved.requested_tasks,
        requested_memory_mb: resolved.requested_memory_mb,
        requested_gpus: resolved.requested_gpus,
        allocation_only: false,
        dependency: args.dependency.or(defaults.dependency),
        array_spec: args.array.or(defaults.array_spec),
        time_limit_secs: resolved.time_limit_secs,
        begin_time,
        exclusive,
        stdout_path: args
            .output
            .map(|path| path.to_string_lossy().to_string())
            .or(defaults.output_path),
        stderr_path: args
            .error
            .map(|path| path.to_string_lossy().to_string())
            .or(defaults.error_path),
        constraint: resolved.constraint,
        cpu_bind: None,
        export_env,
        open_mode,
        warning_signal,
        requeue,
    };

    match send_request(&config, &Request::SubmitBatch(request))? {
        Response::Submitted { job_id } => {
            if args.parsable {
                println!("{job_id}");
            } else {
                println!("Submitted batch job {job_id}");
            }
            if args.wait {
                wait_for_submission_completion(&config, job_id)
            } else {
                Ok(())
            }
        }
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response to sbatch: {other:?}"
        ))),
    }
}

fn run_srun(config: AppConfig, args: SrunArgs) -> Result<()> {
    if let Some(job) = current_allocation_job(&config)? {
        if args.no_wait && (args.label || args.unbuffered) {
            return Err(SlotdError::from(
                "--label and --unbuffered are not supported with --no-wait",
            ));
        }
        return run_foreground_step(
            &config,
            &job,
            &args.command,
            ForegroundExecutionOptions {
                record_step: true,
                cpu_bind: args.cpu_bind.as_deref(),
                io: ForegroundIoOptions {
                    stdout_path: args.output.as_deref(),
                    stderr_path: args.error.as_deref(),
                    label_output: args.label,
                    unbuffered: args.unbuffered,
                },
            },
        );
    }

    let resolved = args.resources.resolve(&config, None)?;
    let command_name = args
        .command
        .first()
        .map(|value| command_basename(value))
        .unwrap_or_else(|| "srun".to_string());
    let command_override = shell_join(&args.command);

    if !args.no_wait {
        return run_interactive_srun(
            &config,
            InteractiveRunSpec {
                name: resolved.job_name.or(Some(command_name)),
                partition: resolved.partition,
                cwd: resolved.cwd,
                requested_cpus: resolved.requested_cpus,
                requested_tasks: resolved.requested_tasks,
                requested_memory_mb: resolved.requested_memory_mb,
                requested_gpus: resolved.requested_gpus,
                time_limit_secs: resolved.time_limit_secs,
                constraint: resolved.constraint,
                cpu_bind: args.cpu_bind,
                label_output: args.label,
                unbuffered: args.unbuffered,
                immediate: args.immediate,
                stdout_path: args.output,
                stderr_path: args.error,
                command: args.command,
            },
        );
    }
    if args.label || args.unbuffered {
        return Err(SlotdError::from(
            "--label and --unbuffered are not supported with --no-wait",
        ));
    }

    let script_body = format!("#!/usr/bin/env bash\nexec {}\n", command_override);

    let request = SubmitRequest {
        name: resolved.job_name.or_else(|| Some(command_name.clone())),
        user_name: current_user_name(),
        partition: resolved.partition,
        cwd: resolved.cwd,
        script_name: command_name,
        script_body,
        command_override: Some(command_override),
        requested_cpus: resolved.requested_cpus,
        requested_tasks: resolved.requested_tasks,
        requested_memory_mb: resolved.requested_memory_mb,
        requested_gpus: resolved.requested_gpus,
        allocation_only: false,
        dependency: None,
        array_spec: None,
        time_limit_secs: resolved.time_limit_secs,
        begin_time: None,
        exclusive: false,
        stdout_path: args
            .output
            .clone()
            .map(|path| path.to_string_lossy().to_string()),
        stderr_path: args
            .error
            .clone()
            .map(|path| path.to_string_lossy().to_string()),
        constraint: resolved.constraint,
        cpu_bind: args.cpu_bind,
        export_env: Vec::new(),
        open_mode: OpenMode::Truncate,
        warning_signal: None,
        requeue: false,
    };

    let job_id = match send_request(
        &config,
        &Request::SubmitRun {
            request,
            immediate: args.immediate,
        },
    )? {
        Response::Submitted { job_id } => job_id,
        Response::Error { message } => return Err(SlotdError::from(message)),
        other => {
            return Err(SlotdError::from(format!(
                "unexpected response to srun: {other:?}"
            )));
        }
    };

    if args.no_wait {
        println!("Submitted run job {job_id}");
        return Ok(());
    }

    Ok(())
}

fn run_salloc(config: AppConfig, args: SallocArgs) -> Result<()> {
    let resolved = args.resources.resolve(&config, None)?;
    let command = if args.command.is_empty() {
        vec![std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())]
    } else {
        args.command
    };
    let command_name = command
        .first()
        .map(|value| command_basename(value))
        .unwrap_or_else(|| "salloc".to_string());

    let request = SubmitRequest {
        name: resolved.job_name.or_else(|| Some("salloc".to_string())),
        user_name: current_user_name(),
        partition: resolved.partition,
        cwd: resolved.cwd.clone(),
        script_name: command_name.clone(),
        script_body: String::new(),
        command_override: Some(shell_join(&command)),
        requested_cpus: resolved.requested_cpus,
        requested_tasks: resolved.requested_tasks,
        requested_memory_mb: resolved.requested_memory_mb,
        requested_gpus: resolved.requested_gpus,
        allocation_only: true,
        dependency: None,
        array_spec: None,
        time_limit_secs: resolved.time_limit_secs,
        begin_time: None,
        exclusive: false,
        stdout_path: None,
        stderr_path: None,
        constraint: resolved.constraint,
        cpu_bind: None,
        export_env: Vec::new(),
        open_mode: OpenMode::Truncate,
        warning_signal: None,
        requeue: false,
    };

    let job_id = match send_request(
        &config,
        &Request::SubmitAlloc {
            request,
            immediate: args.immediate,
        },
    )? {
        Response::Submitted { job_id } => job_id,
        Response::Error { message } => return Err(SlotdError::from(message)),
        other => {
            return Err(SlotdError::from(format!(
                "unexpected response to salloc: {other:?}"
            )));
        }
    };

    println!("Granted job allocation {job_id}");
    let job = wait_for_job_running(&config, job_id)?;
    run_foreground_allocation(&config, &job, &command)
}

fn run_squeue(config: AppConfig, args: SqueueArgs) -> Result<()> {
    let fields = if args.start && args.format.is_none() {
        vec![
            crate::output::SqueueField::JobId,
            crate::output::SqueueField::Partition,
            crate::output::SqueueField::Name,
            crate::output::SqueueField::User,
            crate::output::SqueueField::StateCompact,
            crate::output::SqueueField::StartTime,
            crate::output::SqueueField::NodeListReason,
        ]
    } else {
        parse_squeue_fields(args.format.as_deref(), args.long).map_err(SlotdError::from)?
    };
    let state_filter = if let Some(values) = args.states {
        Some(parse_states(values)?)
    } else if args.all {
        None
    } else {
        Some(vec![JobState::Pending, JobState::Running])
    };

    match send_request(
        &config,
        &Request::ListJobs {
            states: state_filter,
            ids: args.jobs,
            user_name: args.user,
            partitions: args.partitions,
        },
    )? {
        Response::Jobs { jobs } => {
            let jobs = sort_squeue_jobs(jobs, args.sort.as_deref());
            if args.start {
                let start_times = estimate_start_times(&config, &jobs);
                print_squeue_jobs_with_start_times(
                    &config,
                    &jobs,
                    &fields,
                    &start_times,
                    args.noheader,
                    args.array,
                );
            } else if args.array {
                print_squeue_jobs_with_options(&config, &jobs, &fields, args.noheader, true);
            } else {
                print_squeue_jobs(&config, &jobs, &fields, args.noheader);
            }
            Ok(())
        }
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response to squeue: {other:?}"
        ))),
    }
}

fn run_scontrol(config: AppConfig, args: ScontrolArgs) -> Result<()> {
    if !args.entity.eq_ignore_ascii_case("job") {
        return Err(SlotdError::from(
            "supported syntax: scontrol <action> job <job_id>",
        ));
    }

    if args.action.eq_ignore_ascii_case("hold") {
        return match send_request(
            &config,
            &Request::HoldJob {
                job_id: args.job_id,
            },
        )? {
            Response::Submitted { .. } => Ok(()),
            Response::Error { message } => Err(SlotdError::from(message)),
            other => Err(SlotdError::from(format!(
                "unexpected response to scontrol hold: {other:?}"
            ))),
        };
    }

    if args.action.eq_ignore_ascii_case("release") {
        return match send_request(
            &config,
            &Request::ReleaseJob {
                job_id: args.job_id,
            },
        )? {
            Response::Submitted { .. } => Ok(()),
            Response::Error { message } => Err(SlotdError::from(message)),
            other => Err(SlotdError::from(format!(
                "unexpected response to scontrol release: {other:?}"
            ))),
        };
    }

    if args.action.eq_ignore_ascii_case("update") {
        let mut name = None;
        let mut partition = None;
        let mut time_limit_secs = None;
        let mut priority = None;
        for update in &args.updates {
            let Some((key, value)) = update.split_once('=') else {
                return Err(SlotdError::from(format!(
                    "invalid update expression: {update}"
                )));
            };
            match key.to_ascii_lowercase().as_str() {
                "jobname" | "name" => name = Some(value.to_string()),
                "partition" => partition = Some(value.to_string()),
                "timelimit" | "time" => time_limit_secs = Some(parse_time_limit_secs(value)?),
                "priority" => {
                    priority = Some(
                        value
                            .parse::<i32>()
                            .map_err(|_| SlotdError::from(format!("invalid priority: {value}")))?,
                    )
                }
                other => return Err(SlotdError::from(format!("unsupported update key: {other}"))),
            }
        }
        if let Some(partition) = partition.as_deref() {
            let job = load_job(&config, args.job_id)?;
            if let Some(constraint) = job.constraint.as_deref() {
                validate_constraint(&config, constraint, partition)?;
            }
        }
        return match send_request(
            &config,
            &Request::UpdateJob {
                job_id: args.job_id,
                name,
                partition,
                time_limit_secs,
                priority,
            },
        )? {
            Response::Submitted { .. } => Ok(()),
            Response::Error { message } => Err(SlotdError::from(message)),
            other => Err(SlotdError::from(format!(
                "unexpected response to scontrol update: {other:?}"
            ))),
        };
    }

    if !args.action.eq_ignore_ascii_case("show") {
        return Err(SlotdError::from(
            "supported syntax: scontrol show|hold|release|update job <job_id>; update keys: JobName, Partition, TimeLimit, Priority",
        ));
    }

    match send_request(
        &config,
        &Request::GetJob {
            job_id: args.job_id,
        },
    )? {
        Response::Job { job } => {
            let Some(job) = *job else {
                return Err(SlotdError::from(format!("job {} not found", args.job_id)));
            };
            let steps = match send_request(
                &config,
                &Request::ListSteps {
                    parent_job_id: job.id,
                },
            )? {
                Response::Jobs { jobs } => jobs,
                Response::Error { message } => return Err(SlotdError::from(message)),
                other => {
                    return Err(SlotdError::from(format!(
                        "unexpected response to scontrol step listing: {other:?}"
                    )));
                }
            };
            print_scontrol_job(&config, &job, &steps);
            Ok(())
        }
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response to scontrol: {other:?}"
        ))),
    }
}

fn run_sacct(config: AppConfig, args: SacctArgs) -> Result<()> {
    let state_filter = args.states.map(parse_states).transpose()?;
    let fields = parse_sacct_fields(args.format.as_deref()).map_err(SlotdError::from)?;
    let start_time = args
        .start_time
        .as_deref()
        .map(parse_time_filter)
        .transpose()?;
    let end_time = args
        .end_time
        .as_deref()
        .map(parse_time_filter)
        .transpose()?;

    match send_request(
        &config,
        &Request::ListAccountingJobs {
            states: state_filter,
            ids: args.jobs,
            user_name: args.user,
            partitions: args.partitions,
            start_time,
            end_time,
        },
    )? {
        Response::Jobs { jobs } => {
            if args.parsable2 {
                print_sacct_jobs_delimited(&config, &jobs, &fields, args.noheader, "|");
            } else {
                print_sacct_jobs(&config, &jobs, &fields, args.noheader);
            }
            Ok(())
        }
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response to sacct: {other:?}"
        ))),
    }
}

fn run_scancel(config: AppConfig, args: ScancelArgs) -> Result<()> {
    let job_id = resolve_job_reference(&config, &args.job_id)?;
    if let Some(signal) = args.signal.as_deref() {
        let signal = parse_signal_name(signal)?;
        return match send_request(&config, &Request::SignalJob { job_id, signal })? {
            Response::Submitted { job_id } => {
                println!("Signaled job {job_id}");
                Ok(())
            }
            Response::Error { message } => Err(SlotdError::from(message)),
            other => Err(SlotdError::from(format!(
                "unexpected response to scancel signal: {other:?}"
            ))),
        };
    }

    match send_request(&config, &Request::Cancel { job_id })? {
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

fn run_sinfo(config: AppConfig, args: SinfoArgs) -> Result<()> {
    let fields = parse_sinfo_fields(args.format.as_deref(), args.long).map_err(SlotdError::from)?;
    match send_request(&config, &Request::NodeInfo)? {
        Response::NodeInfo { info } => {
            let partitions = filter_partitions(info.partitions, args.partitions.as_deref());
            print_sinfo(&config, &partitions, &fields, args.noheader);
            Ok(())
        }
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response to sinfo: {other:?}"
        ))),
    }
}

fn wait_for_job_completion(config: &AppConfig, job_id: i64) -> Result<JobRecord> {
    loop {
        match send_request(config, &Request::GetJob { job_id })? {
            Response::Job { job } => match *job {
                Some(job) if job.state.is_terminal() => return Ok(job),
                Some(_) => thread::sleep(Duration::from_millis(200)),
                None => return Err(SlotdError::from(format!("job {job_id} disappeared"))),
            },
            Response::Error { message } => return Err(SlotdError::from(message)),
            other => {
                return Err(SlotdError::from(format!(
                    "unexpected response while waiting for job {job_id}: {other:?}"
                )));
            }
        }
    }
}

fn load_job(config: &AppConfig, job_id: i64) -> Result<JobRecord> {
    match send_request(config, &Request::GetJob { job_id })? {
        Response::Job { job } => {
            (*job).ok_or_else(|| SlotdError::from(format!("job {job_id} not found")))
        }
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response while loading job {job_id}: {other:?}"
        ))),
    }
}

fn wait_for_submission_completion(config: &AppConfig, job_id: i64) -> Result<()> {
    let job = wait_for_job_completion(config, job_id)?;
    let mut jobs = vec![job.clone()];

    if job.array_job_id == Some(job.id) && job.array_task_count.unwrap_or(1) > 1 {
        loop {
            match send_request(
                config,
                &Request::ListAccountingJobs {
                    states: None,
                    ids: None,
                    user_name: None,
                    partitions: None,
                    start_time: None,
                    end_time: None,
                },
            )? {
                Response::Jobs { jobs: all_jobs } => {
                    jobs = all_jobs
                        .into_iter()
                        .filter(|entry| entry.array_job_id == Some(job.id))
                        .collect();
                    if jobs.len() as u32 >= job.array_task_count.unwrap_or(0)
                        && jobs.iter().all(|entry| entry.state.is_terminal())
                    {
                        break;
                    }
                }
                Response::Error { message } => return Err(SlotdError::from(message)),
                other => {
                    return Err(SlotdError::from(format!(
                        "unexpected response while waiting for array job {job_id}: {other:?}"
                    )));
                }
            }
            thread::sleep(Duration::from_millis(200));
        }
    }

    let failing_job = jobs
        .into_iter()
        .find(|entry| entry.state != JobState::Completed);
    if let Some(job) = failing_job {
        return Err(SlotdError::Exit(job.exit_code.unwrap_or(1)));
    }
    Ok(())
}

fn wait_for_job_running(config: &AppConfig, job_id: i64) -> Result<JobRecord> {
    loop {
        match send_request(config, &Request::GetJob { job_id })? {
            Response::Job { job } => match *job {
                Some(job) if job.state == JobState::Running => return Ok(job),
                Some(job) if job.state.is_terminal() => {
                    return Err(SlotdError::from(format!(
                        "allocation {job_id} ended before it became runnable: {}",
                        job.state.as_str()
                    )));
                }
                Some(_) => thread::sleep(Duration::from_millis(200)),
                None => {
                    return Err(SlotdError::from(format!("allocation {job_id} disappeared")));
                }
            },
            Response::Error { message } => return Err(SlotdError::from(message)),
            other => {
                return Err(SlotdError::from(format!(
                    "unexpected response while waiting for allocation {job_id}: {other:?}"
                )));
            }
        }
    }
}

fn run_foreground_allocation(
    config: &AppConfig,
    job: &JobRecord,
    command: &[String],
) -> Result<()> {
    run_foreground_allocation_with_mode(config, job, command, ForegroundExecutionOptions::default())
}

fn run_foreground_allocation_with_mode(
    config: &AppConfig,
    job: &JobRecord,
    command: &[String],
    options: ForegroundExecutionOptions<'_>,
) -> Result<()> {
    let step_record = if options.record_step {
        Some(start_step_record(config, job, command)?)
    } else {
        None
    };
    let launcher_script = build_multitask_launcher(
        LaunchCommand::Command(command),
        job.requested_tasks,
        options.io.label_output,
    );
    let mut command_builder = Command::new("/bin/bash");
    command_builder.arg("-lc").arg(launcher_script);
    command_builder.current_dir(&job.cwd);
    command_builder.stdin(Stdio::inherit());
    let io_state = configure_foreground_stdio(
        &mut command_builder,
        &job.cwd,
        options.io.stdout_path,
        options.io.stderr_path,
        false,
        options.io.unbuffered,
    )?;
    apply_slurm_env(
        &mut command_builder,
        config,
        step_record.as_ref().unwrap_or(job),
    );
    let cpu_ids = resolve_cpu_bind_ids(
        options.cpu_bind.or(step_record
            .as_ref()
            .and_then(|step| step.cpu_bind.as_deref())),
        config.total_cpus,
        job.requested_cpus
            .saturating_mul(job.requested_tasks)
            .max(1),
    )?;
    unsafe {
        command_builder.pre_exec(|| {
            nix::unistd::setsid().map_err(std::io::Error::other)?;
            Ok(())
        });
    }
    if let Some(cpu_ids) = cpu_ids {
        unsafe {
            command_builder.pre_exec(move || {
                apply_cpu_affinity(&cpu_ids).map_err(std::io::Error::other)?;
                Ok(())
            });
        }
    }
    let mut child = command_builder.spawn()?;
    drop(command_builder);
    let pid = child.id() as i32;
    let pgid = pid;
    let local_cgroup = match setup_local_cgroup(
        config,
        step_record.as_ref().map(|step| step.id).unwrap_or(job.id),
        step_record.as_ref().unwrap_or(job),
        pid,
    ) {
        Ok(path) => path,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    match send_request(
        config,
        &Request::AdoptAllocation {
            job_id: job.id,
            pid,
            pgid,
        },
    )? {
        Response::Submitted { .. } => {}
        Response::Error { message } => return Err(SlotdError::from(message)),
        other => {
            return Err(SlotdError::from(format!(
                "unexpected response while adopting allocation {}: {other:?}",
                job.id
            )));
        }
    }

    let step_job_id = if let Some(step) = &step_record {
        match send_request(
            config,
            &Request::AdoptAllocation {
                job_id: step.id,
                pid,
                pgid,
            },
        )? {
            Response::Submitted { .. } => Some(step.id),
            Response::Error { message } => return Err(SlotdError::from(message)),
            other => {
                return Err(SlotdError::from(format!(
                    "unexpected response while adopting step {}: {other:?}",
                    step.id
                )));
            }
        }
    } else {
        None
    };

    let status = child.wait()?;
    io_state.finish()?;
    let exit_code = status.code();
    let term_signal = exit_signal(&status);
    let (state, reason) = allocation_terminal_state(
        exit_code,
        term_signal,
        local_cgroup
            .as_deref()
            .map(local_cgroup_oomed)
            .unwrap_or(false),
    );
    match send_request(
        config,
        &Request::FinishAllocation {
            job_id: job.id,
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
                "unexpected response while finishing allocation {}: {other:?}",
                job.id
            )));
        }
    }

    if let Some(step_job_id) = step_job_id {
        match send_request(
            config,
            &Request::FinishAllocation {
                job_id: step_job_id,
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
                    step_job_id
                )));
            }
        }
    }
    cleanup_local_cgroup(local_cgroup.as_deref());

    match state {
        JobState::Completed => Ok(()),
        _ => Err(SlotdError::Exit(exit_code.unwrap_or(1))),
    }
}

struct InteractiveRunSpec {
    name: Option<String>,
    partition: String,
    cwd: String,
    requested_cpus: u32,
    requested_tasks: u32,
    requested_memory_mb: u64,
    requested_gpus: u32,
    time_limit_secs: Option<u64>,
    constraint: Option<String>,
    cpu_bind: Option<String>,
    label_output: bool,
    unbuffered: bool,
    immediate: bool,
    stdout_path: Option<PathBuf>,
    stderr_path: Option<PathBuf>,
    command: Vec<String>,
}

fn run_interactive_srun(config: &AppConfig, spec: InteractiveRunSpec) -> Result<()> {
    let cpu_bind = spec.cpu_bind.clone();
    let request = SubmitRequest {
        name: spec.name.or_else(|| Some("srun".to_string())),
        user_name: current_user_name(),
        partition: spec.partition,
        cwd: spec.cwd,
        script_name: "srun".to_string(),
        script_body: String::new(),
        command_override: Some(shell_join(&spec.command)),
        requested_cpus: spec.requested_cpus,
        requested_tasks: spec.requested_tasks,
        requested_memory_mb: spec.requested_memory_mb,
        requested_gpus: spec.requested_gpus,
        allocation_only: true,
        dependency: None,
        array_spec: None,
        time_limit_secs: spec.time_limit_secs,
        begin_time: None,
        exclusive: false,
        stdout_path: None,
        stderr_path: None,
        constraint: spec.constraint,
        cpu_bind,
        export_env: Vec::new(),
        open_mode: OpenMode::Truncate,
        warning_signal: None,
        requeue: false,
    };

    let job_id = match send_request(
        config,
        &Request::SubmitAlloc {
            request,
            immediate: spec.immediate,
        },
    )? {
        Response::Submitted { job_id } => job_id,
        Response::Error { message } => {
            let message =
                if message == "resources are not currently available for --immediate salloc" {
                    "resources are not currently available for --immediate srun".to_string()
                } else {
                    message
                };
            return Err(SlotdError::from(message));
        }
        other => {
            return Err(SlotdError::from(format!(
                "unexpected response to interactive srun: {other:?}"
            )));
        }
    };

    let job = wait_for_job_running(config, job_id)?;
    run_foreground_allocation_with_mode(
        config,
        &job,
        &spec.command,
        ForegroundExecutionOptions {
            record_step: true,
            cpu_bind: spec.cpu_bind.as_deref(),
            io: ForegroundIoOptions {
                stdout_path: spec.stdout_path.as_deref(),
                stderr_path: spec.stderr_path.as_deref(),
                label_output: spec.label_output,
                unbuffered: spec.unbuffered,
            },
        },
    )
}

fn start_step_record(
    config: &AppConfig,
    parent: &JobRecord,
    command: &[String],
) -> Result<JobRecord> {
    let step_job_id = match send_request(
        config,
        &Request::StartStep {
            parent_job_id: parent.id,
            name: command_basename(&command[0]),
            command: shell_join(command),
            cwd: parent.cwd.clone(),
            user_name: current_user_name(),
        },
    )? {
        Response::Submitted { job_id } => job_id,
        Response::Error { message } => return Err(SlotdError::from(message)),
        other => {
            return Err(SlotdError::from(format!(
                "unexpected response while starting step for job {}: {other:?}",
                parent.id
            )));
        }
    };

    match send_request(
        config,
        &Request::GetJob {
            job_id: step_job_id,
        },
    )? {
        Response::Job { job } => {
            (*job).ok_or_else(|| SlotdError::from(format!("step {step_job_id} disappeared")))
        }
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response while loading step {}: {other:?}",
            step_job_id
        ))),
    }
}

fn apply_slurm_env(command: &mut Command, config: &AppConfig, job: &JobRecord) {
    command.env(
        "SLURM_JOB_ID",
        job.parent_job_id.unwrap_or(job.id).to_string(),
    );
    command.env("SLURM_JOB_NAME", &job.name);
    command.env("SLURM_JOB_PARTITION", &job.partition);
    command.env("SLURM_JOB_NODELIST", &config.hostname);
    command.env("SLURM_SUBMIT_DIR", &job.cwd);
    command.env("SLURM_NTASKS", job.requested_tasks.max(1).to_string());
    command.env("SLURM_CPUS_PER_TASK", job.requested_cpus.to_string());
    if let Some(array_job_id) = job.array_job_id {
        command.env("SLURM_ARRAY_JOB_ID", array_job_id.to_string());
    }
    if let Some(array_task_id) = job.array_task_id {
        command.env("SLURM_ARRAY_TASK_ID", array_task_id.to_string());
    }
    command.env("SLURM_STEP_ID", job.step_id.unwrap_or(0).to_string());
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

enum ForegroundIoState {
    Direct,
    Streamed(Vec<std::thread::JoinHandle<Result<()>>>),
}

impl ForegroundIoState {
    fn finish(self) -> Result<()> {
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

fn configure_foreground_stdio(
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

fn resolve_cpu_bind_ids(
    value: Option<&str>,
    total_cpus: u32,
    requested_cpus: u32,
) -> Result<Option<Vec<usize>>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let normalized = value.to_ascii_lowercase();
    if normalized == "none" {
        return Ok(None);
    }
    if normalized == "cores" {
        let limit = requested_cpus.min(total_cpus).max(1);
        return Ok(Some((0..limit as usize).collect()));
    }
    if let Some(list) = normalized.strip_prefix("map_cpu:") {
        let mut cpus = Vec::new();
        for part in list
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
        {
            let cpu = part
                .parse::<usize>()
                .map_err(|_| SlotdError::from(format!("invalid cpu-bind cpu id: {part}")))?;
            if cpu >= total_cpus as usize {
                return Err(SlotdError::from(format!(
                    "cpu-bind cpu id {cpu} exceeds available CPUs"
                )));
            }
            cpus.push(cpu);
        }
        if cpus.is_empty() {
            return Err(SlotdError::from(
                "cpu-bind map_cpu requires at least one CPU id",
            ));
        }
        cpus.sort_unstable();
        cpus.dedup();
        return Ok(Some(cpus));
    }
    Err(SlotdError::from(format!(
        "unsupported cpu-bind value: {value}; supported: none, cores, map_cpu:<ids>"
    )))
}

fn apply_cpu_affinity(cpu_ids: &[usize]) -> Result<()> {
    let mut cpu_set = nix::sched::CpuSet::new();
    for &cpu_id in cpu_ids {
        cpu_set
            .set(cpu_id)
            .map_err(|error| SlotdError::from(error.to_string()))?;
    }
    nix::sched::sched_setaffinity(nix::unistd::Pid::from_raw(0), &cpu_set)
        .map_err(|error| SlotdError::from(error.to_string()))
}

fn current_allocation_job(config: &AppConfig) -> Result<Option<JobRecord>> {
    let Ok(value) = std::env::var("SLURM_JOB_ID") else {
        return Ok(None);
    };
    let Ok(job_id) = value.parse::<i64>() else {
        return Ok(None);
    };

    match send_request(config, &Request::GetJob { job_id })? {
        Response::Job { job } => {
            Ok((*job).filter(|job| job.state == JobState::Running && job.allocation_only))
        }
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response while loading current allocation: {other:?}"
        ))),
    }
}

fn run_foreground_step(
    config: &AppConfig,
    job: &JobRecord,
    command: &[String],
    options: ForegroundExecutionOptions<'_>,
) -> Result<()> {
    let step = start_step_record(config, job, command)?;
    let launcher_script = build_multitask_launcher(
        LaunchCommand::Command(command),
        step.requested_tasks.max(1),
        options.io.label_output,
    );
    let mut command_builder = Command::new("/bin/bash");
    command_builder.arg("-lc").arg(launcher_script);
    command_builder.current_dir(&job.cwd);
    command_builder.stdin(Stdio::inherit());
    let io_state = configure_foreground_stdio(
        &mut command_builder,
        &job.cwd,
        options.io.stdout_path,
        options.io.stderr_path,
        false,
        options.io.unbuffered,
    )?;
    apply_slurm_env(&mut command_builder, config, &step);
    let cpu_ids = resolve_cpu_bind_ids(
        options.cpu_bind.or(step.cpu_bind.as_deref()),
        config.total_cpus,
        job.requested_cpus
            .saturating_mul(job.requested_tasks)
            .max(1),
    )?;
    unsafe {
        command_builder.pre_exec(|| {
            nix::unistd::setsid().map_err(std::io::Error::other)?;
            Ok(())
        });
    }
    if let Some(cpu_ids) = cpu_ids {
        unsafe {
            command_builder.pre_exec(move || {
                apply_cpu_affinity(&cpu_ids).map_err(std::io::Error::other)?;
                Ok(())
            });
        }
    }
    let mut child = command_builder.spawn()?;
    drop(command_builder);
    let pid = child.id() as i32;
    let pgid = pid;
    let local_cgroup = match setup_local_cgroup(config, step.id, &step, pid) {
        Ok(path) => path,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    match send_request(
        config,
        &Request::AdoptAllocation {
            job_id: step.id,
            pid,
            pgid,
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
    let status = child.wait()?;
    io_state.finish()?;
    let exit_code = status.code();
    let term_signal = exit_signal(&status);
    let (state, reason) = allocation_terminal_state(
        exit_code,
        term_signal,
        local_cgroup
            .as_deref()
            .map(local_cgroup_oomed)
            .unwrap_or(false),
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
    cleanup_local_cgroup(local_cgroup.as_deref());
    match status.code() {
        Some(0) => Ok(()),
        Some(code) => Err(SlotdError::Exit(code)),
        None => Err(SlotdError::Exit(1)),
    }
}

fn print_scontrol_job(config: &AppConfig, job: &JobRecord, steps: &[JobRecord]) {
    let dependency = job.dependency.as_deref().unwrap_or("(null)");
    let reason = job.state_reason.as_deref().unwrap_or("(null)");
    let stdout = if job.stdout_path.is_empty() {
        "(null)"
    } else {
        &job.stdout_path
    };
    let stderr = if job.stderr_path.is_empty() {
        "(null)"
    } else {
        &job.stderr_path
    };
    let node_list = if matches!(job.state, JobState::Pending) {
        "(null)".to_string()
    } else {
        config.hostname.clone()
    };
    let time_limit = job
        .time_limit_secs
        .map(|value| format_duration_secs(value as i64))
        .unwrap_or_else(|| "UNLIMITED".to_string());
    let array = match (job.array_job_id, job.array_task_id) {
        (Some(array_job_id), Some(array_task_id)) => format!("{array_job_id}_{array_task_id}"),
        _ => "(null)".to_string(),
    };
    let req_tres = format_job_req_tres(job);
    let alloc_tres = format_job_alloc_tres(config, job);
    let req_gres = if job.requested_gpus > 0 {
        format!("gpu:{}", job.requested_gpus)
    } else {
        "(null)".to_string()
    };
    println!(
        "JobId={} JobName={} UserId={}({}) Partition={} State={} Reason={}",
        job.id,
        job.name,
        job.user_name,
        job.user_name,
        job.partition,
        job.state.as_str(),
        reason
    );
    println!(
        "   NumTasks={} CPUs/Task={} ReqMem={}MB ReqGRES={} TimeLimit={} BeginTime={} Dependency={} Exclusive={}",
        job.requested_tasks,
        job.requested_cpus,
        job.requested_memory_mb,
        req_gres,
        time_limit,
        format_optional_timestamp(job.begin_time),
        dependency,
        if job.exclusive { "Yes" } else { "No" }
    );
    println!(
        "   SubmitTime={} StartTime={} EndTime={} ExitCode={} ArrayTask={} BatchFlag={}",
        format_timestamp(job.submit_time),
        format_optional_timestamp(job.start_time),
        format_optional_timestamp(job.end_time),
        format_exit_status(job),
        array,
        if job.allocation_only { 0 } else { 1 }
    );
    println!(
        "   WorkDir={} Command={} StdOut={} StdErr={} NodeList={}",
        job.cwd, job.command, stdout, stderr, node_list
    );
    println!(
        "   ReqTRES={} AllocTRES={} MaxRSS={}",
        req_tres,
        alloc_tres,
        job.max_rss_kb
            .map(|value| format!("{value}K"))
            .unwrap_or_else(|| "(null)".to_string())
    );
    if !steps.is_empty() {
        let summary = steps
            .iter()
            .map(|step| {
                format!(
                    "{}:{}:{}",
                    step.step_id.unwrap_or(0),
                    step.state.as_str(),
                    step.command
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        println!("   Steps={summary}");
    }
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

fn exit_signal(status: &std::process::ExitStatus) -> Option<i32> {
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

fn allocation_terminal_state(
    exit_code: Option<i32>,
    term_signal: Option<i32>,
    cgroup_oom: bool,
) -> (JobState, &'static str) {
    if cgroup_oom {
        return (JobState::OutOfMemory, "OutOfMemory");
    }
    match (exit_code, term_signal) {
        (Some(0), None) => (JobState::Completed, "Completed"),
        (Some(_), None) => (JobState::Failed, "NonZeroExitCode"),
        (_, Some(_)) => (JobState::Failed, "Signal"),
        _ => (JobState::Failed, "UnknownFailure"),
    }
}

fn current_dir_string() -> String {
    std::env::current_dir()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string())
}

fn parse_states(values: Vec<String>) -> Result<Vec<JobState>> {
    values
        .into_iter()
        .map(|value| parse_state(&value))
        .collect()
}

fn parse_begin_time(value: &str) -> Result<i64> {
    let trimmed = value.trim();
    if let Some(offset) = trimmed.strip_prefix("now+") {
        return Ok(now_ts().saturating_add(parse_time_limit_secs(offset)? as i64));
    }
    parse_time_filter(trimmed)
}

fn validate_constraint(config: &AppConfig, value: &str, partition: &str) -> Result<()> {
    if config.matches_constraint(value, partition) {
        Ok(())
    } else {
        Err(SlotdError::from(format!(
            "constraint {value:?} does not match local features for partition {partition}"
        )))
    }
}

fn load_sbatch_env_overrides() -> SbatchEnvOverrides {
    load_sbatch_env_overrides_with(|name| std::env::var(name).ok())
}

fn load_sbatch_env_overrides_with<F>(get: F) -> SbatchEnvOverrides
where
    F: Fn(&str) -> Option<String>,
{
    let directives = BatchDirectives {
        job_name: get("SBATCH_JOB_NAME"),
        partition: get("SBATCH_PARTITION"),
        cpus_per_task: get("SBATCH_CPUS_PER_TASK").and_then(|value| value.parse().ok()),
        ntasks: get("SBATCH_NTASKS").and_then(|value| value.parse().ok()),
        mem_mb: get("SBATCH_MEM").and_then(|value| parse_mem_mb(&value).ok()),
        time_limit_secs: get("SBATCH_TIME").and_then(|value| parse_time_limit_secs(&value).ok()),
        gpus: get("SBATCH_GPUS").and_then(|value| value.parse().ok()),
        constraint: get("SBATCH_CONSTRAINT"),
        begin: get("SBATCH_BEGIN"),
        exclusive: get("SBATCH_EXCLUSIVE")
            .map(|value| parse_env_flag(&value))
            .unwrap_or(false),
        requeue: get("SBATCH_REQUEUE")
            .map(|value| parse_env_flag(&value))
            .unwrap_or(false),
        output_path: get("SBATCH_OUTPUT"),
        error_path: get("SBATCH_ERROR"),
        chdir: get("SBATCH_CHDIR"),
        dependency: get("SBATCH_DEPENDENCY"),
        array_spec: get("SBATCH_ARRAY_INX"),
    };
    let exclusive = directives.exclusive;
    let requeue = directives.requeue;

    SbatchEnvOverrides {
        directives,
        export: get("SBATCH_EXPORT"),
        export_file: get("SBATCH_EXPORT_FILE").map(PathBuf::from),
        open_mode: get("SBATCH_OPEN_MODE"),
        signal: get("SBATCH_SIGNAL"),
        begin: get("SBATCH_BEGIN"),
        exclusive,
        requeue,
    }
}

fn merge_batch_directives(
    directives: &BatchDirectives,
    overrides: &BatchDirectives,
) -> BatchDirectives {
    BatchDirectives {
        job_name: overrides
            .job_name
            .clone()
            .or_else(|| directives.job_name.clone()),
        partition: overrides
            .partition
            .clone()
            .or_else(|| directives.partition.clone()),
        cpus_per_task: overrides.cpus_per_task.or(directives.cpus_per_task),
        ntasks: overrides.ntasks.or(directives.ntasks),
        mem_mb: overrides.mem_mb.or(directives.mem_mb),
        gpus: overrides.gpus.or(directives.gpus),
        constraint: overrides
            .constraint
            .clone()
            .or_else(|| directives.constraint.clone()),
        begin: overrides.begin.clone().or_else(|| directives.begin.clone()),
        exclusive: overrides.exclusive || directives.exclusive,
        requeue: overrides.requeue || directives.requeue,
        time_limit_secs: overrides.time_limit_secs.or(directives.time_limit_secs),
        dependency: overrides
            .dependency
            .clone()
            .or_else(|| directives.dependency.clone()),
        array_spec: overrides
            .array_spec
            .clone()
            .or_else(|| directives.array_spec.clone()),
        output_path: overrides
            .output_path
            .clone()
            .or_else(|| directives.output_path.clone()),
        error_path: overrides
            .error_path
            .clone()
            .or_else(|| directives.error_path.clone()),
        chdir: overrides.chdir.clone().or_else(|| directives.chdir.clone()),
    }
}

fn resolve_export_env(
    export: Option<&str>,
    export_file: Option<&Path>,
) -> Result<Vec<(String, String)>> {
    let mut env = Vec::<(String, String)>::new();
    if let Some(path) = export_file {
        let contents = fs::read_to_string(path)?;
        env.extend(parse_export_file_contents(&contents)?);
    }
    if let Some(spec) = export {
        env = resolve_export_spec(spec, &env)?;
    }
    env.sort_by(|a, b| a.0.cmp(&b.0));
    env.dedup_by(|a, b| a.0 == b.0);
    Ok(env)
}

fn parse_env_flag(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes"
    )
}

fn parse_export_file_contents(contents: &str) -> Result<Vec<(String, String)>> {
    let mut env = Vec::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(SlotdError::from(format!(
                "invalid export-file entry: {line}"
            )));
        };
        validate_env_name(key)?;
        env.push((key.to_string(), value.to_string()));
    }
    Ok(env)
}

fn resolve_export_spec(spec: &str, seed: &[(String, String)]) -> Result<Vec<(String, String)>> {
    let trimmed = spec.trim();
    if trimmed.eq_ignore_ascii_case("none") {
        return Ok(Vec::new());
    }

    let mut env = if trimmed.eq_ignore_ascii_case("all") || trimmed.starts_with("ALL,") {
        std::env::vars().collect::<Vec<_>>()
    } else {
        seed.to_vec()
    };
    let entries = trimmed
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .collect::<Vec<_>>();

    for entry in entries {
        if entry.eq_ignore_ascii_case("all") {
            continue;
        }
        if entry.eq_ignore_ascii_case("none") {
            env.clear();
            continue;
        }
        if let Some((key, value)) = entry.split_once('=') {
            validate_env_name(key)?;
            upsert_env_pair(&mut env, key, value);
            continue;
        }
        validate_env_name(entry)?;
        let value = std::env::var(entry).unwrap_or_default();
        upsert_env_pair(&mut env, entry, &value);
    }

    Ok(env)
}

fn upsert_env_pair(env: &mut Vec<(String, String)>, key: &str, value: &str) {
    if let Some(existing) = env.iter_mut().find(|(name, _)| name == key) {
        existing.1 = value.to_string();
    } else {
        env.push((key.to_string(), value.to_string()));
    }
}

fn validate_env_name(value: &str) -> Result<()> {
    if value.is_empty()
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        || value.chars().next().is_some_and(|ch| ch.is_ascii_digit())
    {
        return Err(SlotdError::from(format!(
            "invalid environment variable name: {value}"
        )));
    }
    Ok(())
}

fn parse_warning_signal(value: &str) -> Result<WarningSignal> {
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

fn parse_signal_name(value: &str) -> Result<i32> {
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

fn estimate_start_times(
    config: &AppConfig,
    jobs: &[JobRecord],
) -> std::collections::HashMap<i64, String> {
    let mut result = std::collections::HashMap::new();
    let now = now_ts();
    let running_jobs = jobs
        .iter()
        .filter(|job| job.state == JobState::Running && job.parent_job_id.is_none())
        .collect::<Vec<_>>();
    let used_cpus = running_jobs
        .iter()
        .map(|job| job.requested_cpus.saturating_mul(job.requested_tasks))
        .sum::<u32>();
    let used_memory_mb = running_jobs
        .iter()
        .map(|job| job.requested_memory_mb)
        .sum::<u64>();
    let used_gpus = running_jobs
        .iter()
        .map(|job| job.requested_gpus)
        .sum::<u32>();
    let running_release = running_jobs
        .iter()
        .filter_map(|job| Some(job.start_time?.saturating_add(job.time_limit_secs? as i64)))
        .max();

    for job in jobs {
        let value = match job.state {
            JobState::Running => job.start_time.map(format_timestamp),
            JobState::Pending => {
                let begin_time = job.begin_time.filter(|value| *value > now);
                let fits_now = job.requested_cpus.saturating_mul(job.requested_tasks)
                    <= config.total_cpus.saturating_sub(used_cpus)
                    && job.requested_memory_mb
                        <= config.total_memory_mb.saturating_sub(used_memory_mb)
                    && job.requested_gpus <= config.total_gpus.saturating_sub(used_gpus);
                if fits_now {
                    Some(format_timestamp(begin_time.unwrap_or(now)))
                } else {
                    match (running_release, begin_time) {
                        (Some(release), Some(begin)) => Some(format_timestamp(release.max(begin))),
                        (Some(release), None) => Some(format_timestamp(release)),
                        (None, Some(begin)) => Some(format_timestamp(begin)),
                        (None, None) => None,
                    }
                }
            }
            _ => None,
        };
        result.insert(job.id, value.unwrap_or_else(|| "N/A".to_string()));
    }

    result
}

fn now_ts() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn parse_state(value: &str) -> Result<JobState> {
    let normalized = value.trim().to_ascii_uppercase();
    match normalized.as_str() {
        "PD" | "PENDING" => Ok(JobState::Pending),
        "R" | "RUNNING" => Ok(JobState::Running),
        "CG" | "COMPLETING" => Ok(JobState::Completing),
        "CD" | "COMPLETED" => Ok(JobState::Completed),
        "F" | "FAILED" => Ok(JobState::Failed),
        "CA" | "CANCELLED" => Ok(JobState::Cancelled),
        "TO" | "TIMEOUT" => Ok(JobState::Timeout),
        "OOM" | "OUT_OF_MEMORY" => Ok(JobState::OutOfMemory),
        _ => Err(SlotdError::from(format!("unknown state: {value}"))),
    }
}

fn parse_time_filter(value: &str) -> Result<i64> {
    if let Ok(ts) = value.parse::<i64>() {
        return Ok(ts);
    }

    if let Some((year, month, day, hour, minute, second)) = parse_datetime_parts(value) {
        return datetime_to_epoch(year, month, day, hour, minute, second);
    }

    Err(SlotdError::from(format!(
        "unsupported time format: {value}"
    )))
}

fn filter_partitions(
    partitions: Vec<crate::job::PartitionInfo>,
    filters: Option<&[String]>,
) -> Vec<crate::job::PartitionInfo> {
    partitions
        .into_iter()
        .filter(|partition| {
            filters
                .map(|filters| filters.iter().any(|filter| filter == &partition.name))
                .unwrap_or(true)
        })
        .collect()
}

fn sort_squeue_jobs(mut jobs: Vec<JobRecord>, sort: Option<&str>) -> Vec<JobRecord> {
    let Some(sort) = sort.map(str::trim).filter(|value| !value.is_empty()) else {
        return jobs;
    };
    let descending = sort.starts_with('-');
    let key = sort.trim_start_matches(['+', '-']);
    jobs.sort_by(|a, b| {
        let ordering = match key.to_ascii_lowercase().as_str() {
            "i" | "jobid" => a.id.cmp(&b.id),
            "p" | "partition" => a.partition.cmp(&b.partition),
            "u" | "user" => a.user_name.cmp(&b.user_name),
            "t" | "state" => a.state.as_str().cmp(b.state.as_str()),
            "m" | "time" => a.start_time.cmp(&b.start_time),
            _ => a.id.cmp(&b.id),
        };
        if descending {
            ordering.reverse()
        } else {
            ordering
        }
    });
    jobs
}

fn resolve_job_reference(config: &AppConfig, value: &str) -> Result<i64> {
    if let Some((parent, step)) = value.split_once('.') {
        let parent_job_id = parent
            .parse::<i64>()
            .map_err(|_| SlotdError::from(format!("invalid job id: {value}")))?;
        let step_id = step
            .parse::<u32>()
            .map_err(|_| SlotdError::from(format!("invalid step id: {value}")))?;
        return match send_request(config, &Request::ListSteps { parent_job_id })? {
            Response::Jobs { jobs } => jobs
                .into_iter()
                .find(|job| job.step_id == Some(step_id))
                .map(|job| job.id)
                .ok_or_else(|| SlotdError::from(format!("unknown step reference: {value}"))),
            Response::Error { message } => Err(SlotdError::from(message)),
            other => Err(SlotdError::from(format!(
                "unexpected response while resolving step reference {value}: {other:?}"
            ))),
        };
    }

    value
        .parse::<i64>()
        .map_err(|_| SlotdError::from(format!("invalid job id: {value}")))
}

fn parse_datetime_parts(value: &str) -> Option<(i32, u32, u32, u32, u32, u32)> {
    if let Some((date, time)) = value.split_once('T').or_else(|| value.split_once(' ')) {
        let (year, month, day) = parse_date(date)?;
        let (hour, minute, second) = parse_time(time)?;
        return Some((year, month, day, hour, minute, second));
    }

    let (year, month, day) = parse_date(value)?;
    Some((year, month, day, 0, 0, 0))
}

fn parse_date(value: &str) -> Option<(i32, u32, u32)> {
    let mut parts = value.split('-');
    let year = parts.next()?.parse().ok()?;
    let month = parts.next()?.parse().ok()?;
    let day = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((year, month, day))
}

fn parse_time(value: &str) -> Option<(u32, u32, u32)> {
    let mut parts = value.split(':');
    let hour = parts.next()?.parse().ok()?;
    let minute = parts.next()?.parse().ok()?;
    let second = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((hour, minute, second))
}

fn datetime_to_epoch(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> Result<i64> {
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return Err(SlotdError::from("invalid date or time"));
    }

    let days = days_from_civil(year, month, day).ok_or_else(|| SlotdError::from("invalid date"))?;
    Ok(days * 86_400 + hour as i64 * 3_600 + minute as i64 * 60 + second as i64)
}

fn days_from_civil(year: i32, month: u32, day: u32) -> Option<i64> {
    if day > days_in_month(year, month)? {
        return None;
    }

    let mut year = year as i64;
    let month = month as i64;
    let day = day as i64;
    year -= if month <= 2 { 1 } else { 0 };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

fn days_in_month(year: i32, month: u32) -> Option<u32> {
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => return None,
    };
    Some(days)
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn format_exit_status(job: &JobRecord) -> String {
    format!(
        "{}:{}",
        job.exit_code.unwrap_or(0),
        job.term_signal.unwrap_or(0)
    )
}

fn format_optional_timestamp(value: Option<i64>) -> String {
    value
        .map(format_timestamp)
        .unwrap_or_else(|| "Unknown".to_string())
}

fn format_timestamp(value: i64) -> String {
    let (year, month, day, hour, minute, second) = civil_from_epoch(value);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}")
}

fn civil_from_epoch(timestamp: i64) -> (i32, u32, u32, u32, u32, u32) {
    let days = timestamp.div_euclid(86_400);
    let secs = timestamp.rem_euclid(86_400) as u32;
    let (year, month, day) = civil_from_days(days);
    let hour = secs / 3_600;
    let minute = (secs % 3_600) / 60;
    let second = secs % 60;
    (year, month, day, hour, minute, second)
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if month <= 2 { 1 } else { 0 };
    (year as i32, month as u32, day as u32)
}

fn format_duration_secs(seconds: i64) -> String {
    let seconds = seconds.max(0) as u64;
    let days = seconds / 86_400;
    let rem = seconds % 86_400;
    let hours = rem / 3_600;
    let minutes = (rem % 3_600) / 60;
    let secs = rem % 60;
    if days > 0 {
        format!("{days}-{hours:02}:{minutes:02}:{secs:02}")
    } else {
        format!("{hours:02}:{minutes:02}:{secs:02}")
    }
}

fn format_job_req_tres(job: &JobRecord) -> String {
    let mut values = vec![
        format!(
            "cpu={}",
            job.requested_cpus.saturating_mul(job.requested_tasks)
        ),
        format!("mem={}M", job.requested_memory_mb),
    ];
    if job.requested_gpus > 0 {
        values.push(format!("gres/gpu={}", job.requested_gpus));
    }
    values.join(",")
}

fn format_job_alloc_tres(config: &AppConfig, job: &JobRecord) -> String {
    let mut values = vec![
        format!(
            "cpu={}",
            job.requested_cpus.saturating_mul(job.requested_tasks)
        ),
        format!("mem={}M", job.requested_memory_mb),
    ];
    if job.state != JobState::Pending {
        values.push("node=1".to_string());
    }
    if config.is_gpu_partition(&job.partition) && job.requested_gpus > 0 {
        values.push(format!("gres/gpu={}", job.requested_gpus));
    }
    values.join(",")
}

fn setup_local_cgroup(
    config: &AppConfig,
    job_id: i64,
    job: &JobRecord,
    pid: i32,
) -> Result<Option<PathBuf>> {
    let Some(base) = &config.cgroup_base else {
        return Ok(None);
    };
    let path = base.join(format!("slotd-{job_id}"));
    std::fs::create_dir_all(&path)?;
    std::fs::write(
        path.join("memory.max"),
        job.requested_memory_mb
            .saturating_mul(1024 * 1024)
            .to_string(),
    )?;
    let requested_cpus = job
        .requested_cpus
        .saturating_mul(job.requested_tasks)
        .max(1);
    let quota = 100_000u64
        .saturating_mul(requested_cpus as u64)
        .checked_div(config.total_cpus.max(1) as u64)
        .unwrap_or(100_000)
        .max(1);
    std::fs::write(path.join("cpu.max"), format!("{quota} 100000"))?;
    std::fs::write(path.join("cgroup.procs"), pid.to_string())?;
    Ok(Some(path))
}

fn local_cgroup_oomed(path: &Path) -> bool {
    let Ok(contents) = std::fs::read_to_string(path.join("memory.events")) else {
        return false;
    };
    contents.lines().any(|line| {
        let mut parts = line.split_whitespace();
        matches!(parts.next(), Some("oom_kill") | Some("oom"))
            && parts
                .next()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(0)
                > 0
    })
}

fn cleanup_local_cgroup(path: Option<&Path>) {
    if let Some(path) = path {
        let _ = std::fs::remove_dir(path);
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use clap::CommandFactory;

    use super::{
        CORE_RESOURCE_LONG_FLAGS, Cli, ResourceArgs, SUPPORTED_ROOT_COMMANDS,
        SUPPORTED_USER_COMMANDS, dispatch_argv0, format_duration_secs, format_timestamp,
        load_sbatch_env_overrides_with, merge_batch_directives, parse_begin_time,
        parse_signal_name, parse_time_filter, parse_warning_signal, resolve_cpu_bind_ids,
        resolve_export_spec,
    };
    use crate::config::AppConfig;
    use crate::job::{JobRecord, JobState, OpenMode};
    use crate::sbatch::BatchDirectives;

    #[test]
    fn argv0_dispatch_inserts_slurm_alias() {
        let argv = vec![OsString::from("squeue"), OsString::from("--noheader")];
        let dispatched = dispatch_argv0(argv);
        assert_eq!(dispatched[1], OsString::from("squeue"));
        assert_eq!(dispatched[2], OsString::from("--noheader"));
    }

    #[test]
    fn phase0_supported_commands_are_explicitly_fixed() {
        let command = Cli::command();
        let names = command
            .get_subcommands()
            .map(|subcommand| subcommand.get_name())
            .collect::<Vec<_>>();
        assert_eq!(names, SUPPORTED_ROOT_COMMANDS);

        let user_commands = names
            .iter()
            .copied()
            .filter(|name| *name != "daemon")
            .collect::<Vec<_>>();
        assert_eq!(user_commands, SUPPORTED_USER_COMMANDS);
    }

    #[test]
    fn phase0_core_resource_flags_are_shared_across_submission_commands() {
        let command = Cli::command();
        for subcommand_name in ["sbatch", "srun", "salloc"] {
            let subcommand = command
                .get_subcommands()
                .find(|subcommand| subcommand.get_name() == subcommand_name)
                .expect("subcommand exists");
            let option_names = subcommand
                .get_arguments()
                .filter_map(|argument| argument.get_long())
                .collect::<Vec<_>>();
            for flag in CORE_RESOURCE_LONG_FLAGS {
                assert!(
                    option_names.contains(flag),
                    "{subcommand_name} is missing shared resource flag --{flag}"
                );
            }
        }
    }

    #[test]
    fn phase0_resource_model_uses_one_shared_defaulting_path() {
        let config = AppConfig::load();
        let args = ResourceArgs {
            job_name: Some("demo".to_string()),
            partition: Some(config.default_partition().to_string()),
            cpus_per_task: Some(4),
            ntasks: Some(2),
            mem: Some("2G".to_string()),
            time: Some("00:30:00".to_string()),
            gpus: Some(0),
            chdir: Some("/tmp".into()),
            constraint: None,
        };

        let resolved = args.resolve(&config, None).expect("resource args resolve");
        assert_eq!(resolved.job_name.as_deref(), Some("demo"));
        assert_eq!(resolved.partition, config.default_partition());
        assert_eq!(resolved.cwd, "/tmp");
        assert_eq!(resolved.requested_cpus, 4);
        assert_eq!(resolved.requested_tasks, 2);
        assert_eq!(resolved.requested_memory_mb, 2048);
        assert_eq!(resolved.requested_gpus, 0);
        assert_eq!(resolved.time_limit_secs, Some(1800));
    }

    #[test]
    fn phase1_cli_resource_values_override_batch_directives() {
        let config = AppConfig::load();
        let directives = BatchDirectives {
            job_name: Some("from-directive".to_string()),
            partition: Some(config.default_partition().to_string()),
            cpus_per_task: Some(2),
            ntasks: Some(3),
            mem_mb: Some(1024),
            gpus: Some(1),
            constraint: None,
            time_limit_secs: Some(600),
            begin: None,
            exclusive: false,
            requeue: false,
            dependency: None,
            array_spec: None,
            output_path: None,
            error_path: None,
            chdir: Some("/directive".to_string()),
        };
        let args = ResourceArgs {
            job_name: Some("from-cli".to_string()),
            partition: Some(config.default_partition().to_string()),
            cpus_per_task: Some(4),
            ntasks: Some(5),
            mem: Some("2G".to_string()),
            time: Some("00:30:00".to_string()),
            gpus: Some(0),
            chdir: Some("/cli".into()),
            constraint: None,
        };

        let resolved = args
            .resolve(&config, Some(&directives))
            .expect("resource args resolve");
        assert_eq!(resolved.job_name.as_deref(), Some("from-cli"));
        assert_eq!(resolved.cwd, "/cli");
        assert_eq!(resolved.requested_cpus, 4);
        assert_eq!(resolved.requested_tasks, 5);
        assert_eq!(resolved.requested_memory_mb, 2048);
        assert_eq!(resolved.requested_gpus, 0);
        assert_eq!(resolved.time_limit_secs, Some(1800));
    }

    #[test]
    fn phase15_sbatch_environment_overrides_directives() {
        let env = load_sbatch_env_overrides_with(|name| match name {
            "SBATCH_PARTITION" => Some("cpu".to_string()),
            "SBATCH_CPUS_PER_TASK" => Some("8".to_string()),
            "SBATCH_TIME" => Some("01:00:00".to_string()),
            "SBATCH_OUTPUT" => Some("from-env.out".to_string()),
            _ => None,
        });
        let directives = BatchDirectives {
            partition: Some("gpu".to_string()),
            cpus_per_task: Some(2),
            time_limit_secs: Some(300),
            output_path: Some("from-directive.out".to_string()),
            ..BatchDirectives::default()
        };
        let merged = merge_batch_directives(&directives, &env.directives);
        assert_eq!(merged.partition.as_deref(), Some("cpu"));
        assert_eq!(merged.cpus_per_task, Some(8));
        assert_eq!(merged.time_limit_secs, Some(3600));
        assert_eq!(merged.output_path.as_deref(), Some("from-env.out"));
    }

    #[test]
    fn phase15_sbatch_environment_keeps_non_overridden_directives() {
        let env = load_sbatch_env_overrides_with(|_| None);
        let directives = BatchDirectives {
            partition: Some("cpu".to_string()),
            cpus_per_task: Some(2),
            output_path: Some("from-directive.out".to_string()),
            ..BatchDirectives::default()
        };
        let merged = merge_batch_directives(&directives, &env.directives);
        assert_eq!(merged.partition.as_deref(), Some("cpu"));
        assert_eq!(merged.cpus_per_task, Some(2));
        assert_eq!(merged.output_path.as_deref(), Some("from-directive.out"));
    }

    #[test]
    fn phase5_sbatch_requeue_environment_overrides_directives() {
        let env = load_sbatch_env_overrides_with(|name| match name {
            "SBATCH_REQUEUE" => Some("yes".to_string()),
            _ => None,
        });
        let directives = BatchDirectives {
            requeue: false,
            ..BatchDirectives::default()
        };
        let merged = merge_batch_directives(&directives, &env.directives);
        assert!(merged.requeue);
    }

    #[test]
    fn phase3_constraint_is_shared_and_validated() {
        let config = AppConfig::load();
        let args = ResourceArgs {
            job_name: None,
            partition: Some(config.default_partition().to_string()),
            cpus_per_task: None,
            ntasks: None,
            mem: None,
            time: None,
            gpus: None,
            chdir: None,
            constraint: Some("cpu".to_string()),
        };
        let resolved = args.resolve(&config, None).expect("resource args resolve");
        assert_eq!(resolved.constraint.as_deref(), Some("cpu"));
    }

    #[test]
    fn phase3_cpu_bind_map_cpu_is_parsed() {
        let cpu_ids = resolve_cpu_bind_ids(Some("map_cpu:0,2,2"), 8, 4)
            .expect("cpu bind")
            .expect("cpu ids");
        assert_eq!(cpu_ids, vec![0, 2]);
    }

    #[test]
    fn phase4_begin_time_supports_now_offset() {
        let before = super::now_ts();
        let begin = parse_begin_time("now+00:10:00").expect("begin time");
        let after = super::now_ts();
        assert!(begin >= before + 600);
        assert!(begin <= after + 600);
    }

    #[test]
    fn parses_date_only_time_filter() {
        assert_eq!(parse_time_filter("1970-01-02").expect("parse date"), 86_400);
    }

    #[test]
    fn parses_full_timestamp_time_filter() {
        assert_eq!(
            parse_time_filter("1970-01-02T03:04:05").expect("parse datetime"),
            97_445
        );
    }

    #[test]
    fn formats_timestamp_and_duration() {
        assert_eq!(format_timestamp(97_445), "1970-01-02T03:04:05");
        assert_eq!(format_duration_secs(3_661), "01:01:01");
    }

    #[test]
    fn phase2_export_spec_none_clears_seed_values() {
        let resolved = resolve_export_spec("NONE", &[("KEEP".to_string(), "value".to_string())])
            .expect("resolve export");
        assert!(resolved.is_empty());
    }

    #[test]
    fn phase2_export_spec_updates_seed_and_adds_assignments() {
        let resolved = resolve_export_spec(
            "FOO=updated,BAR=baz",
            &[("FOO".to_string(), "old".to_string())],
        )
        .expect("resolve export");
        assert_eq!(
            resolved,
            vec![
                ("FOO".to_string(), "updated".to_string()),
                ("BAR".to_string(), "baz".to_string()),
            ]
        );
    }

    #[test]
    fn phase2_warning_signal_parses_batch_prefix_and_offset() {
        let warning = parse_warning_signal("B:USR1@90").expect("warning signal");
        assert_eq!(warning.signal, parse_signal_name("USR1").expect("signal"));
        assert_eq!(warning.seconds_before_end, 90);
    }

    #[test]
    fn phase2_warning_signal_defaults_offset_to_sixty_seconds() {
        let warning = parse_warning_signal("TERM").expect("warning signal");
        assert_eq!(warning.signal, parse_signal_name("TERM").expect("signal"));
        assert_eq!(warning.seconds_before_end, 60);
    }

    #[test]
    fn phase2_signal_parser_accepts_signal_names_and_numbers() {
        assert_eq!(
            parse_signal_name("SIGTERM").expect("named signal"),
            parse_signal_name("TERM").expect("canonical signal")
        );
        assert_eq!(parse_signal_name("15").expect("numeric signal"), 15);
    }

    #[test]
    fn phase2_start_time_estimator_marks_jobs_that_fit_now() {
        let mut config = AppConfig::load();
        config.total_cpus = 8;
        config.total_memory_mb = 16_384;
        config.total_gpus = 1;
        let jobs = vec![JobRecord {
            id: 42,
            parent_job_id: None,
            step_id: None,
            held: false,
            priority: 0,
            array_job_id: None,
            array_task_id: None,
            array_task_count: None,
            array_task_limit: None,
            user_name: "user".to_string(),
            partition: config.default_partition().to_string(),
            name: "pending".to_string(),
            state: JobState::Pending,
            command: "sleep 1".to_string(),
            exit_code: None,
            allocation_only: false,
            dependency: None,
            max_rss_kb: None,
            submit_time: 0,
            start_time: None,
            end_time: None,
            pid: None,
            pgid: None,
            requested_cpus: 2,
            requested_tasks: 1,
            requested_memory_mb: 512,
            requested_gpus: 0,
            assigned_gpu_ids: Vec::new(),
            cwd: "/tmp".to_string(),
            script_path: String::new(),
            stdout_path: "slurm-42.out".to_string(),
            stderr_path: "slurm-42.out".to_string(),
            constraint: None,
            cpu_bind: None,
            state_reason: None,
            term_signal: None,
            time_limit_secs: Some(300),
            begin_time: None,
            exclusive: false,
            export_env: Vec::new(),
            open_mode: OpenMode::Truncate,
            warning_signal: None,
            requeue: false,
            requeue_count: 0,
        }];

        let start_times = super::estimate_start_times(&config, &jobs);
        let start = start_times.get(&42).expect("start time");
        assert_ne!(start, "N/A");
    }
}
