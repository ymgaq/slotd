use std::ffi::OsString;
use std::fs;
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
use crate::job::{JobRecord, JobState, SubmitRequest};
use crate::output::{
    parse_sacct_fields, parse_sinfo_fields, parse_squeue_fields, print_sacct_jobs, print_sinfo,
    print_squeue_jobs,
};
use crate::sbatch::{parse_directives, parse_mem_mb, parse_time_limit_secs};

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
pub struct SbatchArgs {
    #[arg(required_unless_present = "wrap", conflicts_with = "wrap")]
    script: Option<PathBuf>,
    #[arg(long)]
    wrap: Option<String>,
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
    #[arg(long, short = 'o')]
    output: Option<PathBuf>,
    #[arg(long, short = 'e')]
    error: Option<PathBuf>,
    #[arg(long, short = 'D')]
    chdir: Option<PathBuf>,
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
}

#[derive(Debug, Args)]
pub struct ScancelArgs {
    job_id: i64,
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
    #[arg(short = 'n', long = "noheader")]
    noheader: bool,
}

#[derive(Debug, Args)]
pub struct SrunArgs {
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
    #[arg(long, short = 'o')]
    output: Option<PathBuf>,
    #[arg(long, short = 'e')]
    error: Option<PathBuf>,
    #[arg(long, short = 'D')]
    chdir: Option<PathBuf>,
    #[arg(long)]
    immediate: bool,
    #[arg(long)]
    pty: bool,
    #[arg(long, hide = true)]
    no_wait: bool,
    #[arg(required = true, num_args = 1.., trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<String>,
}

#[derive(Debug, Args)]
pub struct SallocArgs {
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
    immediate: bool,
    #[arg(num_args = 0.., trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<String>,
}

#[derive(Debug, Args)]
pub struct SinfoArgs {
    #[arg(short = 'p', long = "partition", value_delimiter = ',')]
    partitions: Option<Vec<String>>,
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

    let cwd = args
        .chdir
        .as_ref()
        .map(|path| path.to_string_lossy().to_string())
        .or(directives.chdir.clone())
        .unwrap_or_else(current_dir_string);
    let partition = args
        .partition
        .clone()
        .or(directives.partition.clone())
        .unwrap_or_else(|| config.default_partition().to_string());
    if !config.has_partition(&partition) {
        return Err(SlotdError::from(format!("unknown partition: {partition}")));
    }
    let requested_cpus = args.cpus_per_task.or(directives.cpus_per_task).unwrap_or(1);
    let requested_tasks = args.ntasks.or(directives.ntasks).unwrap_or(1);
    let requested_memory_mb = match &args.mem {
        Some(value) => parse_mem_mb(value)?,
        None => directives.mem_mb.unwrap_or(512),
    };
    let requested_gpus = args
        .gpus
        .or(directives.gpus)
        .unwrap_or_else(|| config.default_gpus_for_partition(&partition));
    let time_limit_secs = match &args.time {
        Some(value) => Some(parse_time_limit_secs(value)?),
        None => directives.time_limit_secs,
    };

    let request = SubmitRequest {
        name: args.job_name.or(directives.job_name),
        user_name: current_user_name(),
        partition,
        cwd,
        script_name,
        script_body,
        command_override,
        requested_cpus,
        requested_tasks,
        requested_memory_mb,
        requested_gpus,
        allocation_only: false,
        dependency: args.dependency.or(directives.dependency),
        array_spec: args.array.or(directives.array_spec),
        time_limit_secs,
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
        return run_foreground_step(&config, &job, &args.command);
    }

    let cwd = args
        .chdir
        .as_ref()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(current_dir_string);
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
    let requested_tasks = args.ntasks.unwrap_or(1);
    let requested_memory_mb = match args.mem {
        Some(value) => parse_mem_mb(&value)?,
        None => 512,
    };
    let requested_gpus = args
        .gpus
        .unwrap_or_else(|| config.default_gpus_for_partition(&partition));
    let time_limit_secs = args
        .time
        .as_deref()
        .map(parse_time_limit_secs)
        .transpose()?;
    let command_override = shell_join(&args.command);

    if !args.no_wait && (args.pty || (args.output.is_none() && args.error.is_none())) {
        return run_interactive_srun(
            &config,
            InteractiveRunSpec {
                name: args.job_name.or_else(|| Some(command_name)),
                partition,
                cwd,
                requested_cpus,
                requested_tasks,
                requested_memory_mb,
                requested_gpus,
                time_limit_secs,
                immediate: args.immediate,
                command: args.command,
            },
        );
    }

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
        requested_tasks,
        requested_memory_mb,
        requested_gpus,
        allocation_only: false,
        dependency: None,
        array_spec: None,
        time_limit_secs,
        stdout_path: args
            .output
            .clone()
            .map(|path| path.to_string_lossy().to_string()),
        stderr_path: args
            .error
            .clone()
            .map(|path| path.to_string_lossy().to_string()),
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

    let job = wait_for_job_completion(&config, job_id)?;
    replay_srun_output(&job, args.output.is_none(), args.error.is_none())?;

    match job.state {
        JobState::Completed => Ok(()),
        _ => Err(SlotdError::Exit(job.exit_code.unwrap_or(1))),
    }
}

fn run_salloc(config: AppConfig, args: SallocArgs) -> Result<()> {
    let cwd = args
        .chdir
        .as_ref()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(current_dir_string);
    let command = if args.command.is_empty() {
        vec![std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())]
    } else {
        args.command
    };
    let command_name = command
        .first()
        .map(|value| command_basename(value))
        .unwrap_or_else(|| "salloc".to_string());
    let partition = args
        .partition
        .unwrap_or_else(|| config.default_partition().to_string());
    if !config.has_partition(&partition) {
        return Err(SlotdError::from(format!("unknown partition: {partition}")));
    }
    let requested_cpus = args.cpus_per_task.unwrap_or(1);
    let requested_tasks = args.ntasks.unwrap_or(1);
    let requested_memory_mb = match args.mem {
        Some(value) => parse_mem_mb(&value)?,
        None => 512,
    };
    let requested_gpus = args
        .gpus
        .unwrap_or_else(|| config.default_gpus_for_partition(&partition));
    let time_limit_secs = args
        .time
        .as_deref()
        .map(parse_time_limit_secs)
        .transpose()?;

    let request = SubmitRequest {
        name: args.job_name.or_else(|| Some("salloc".to_string())),
        user_name: current_user_name(),
        partition,
        cwd: cwd.clone(),
        script_name: command_name.clone(),
        script_body: String::new(),
        command_override: Some(shell_join(&command)),
        requested_cpus,
        requested_tasks,
        requested_memory_mb,
        requested_gpus,
        allocation_only: true,
        dependency: None,
        array_spec: None,
        time_limit_secs,
        stdout_path: None,
        stderr_path: None,
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
    let fields = parse_squeue_fields(args.format.as_deref()).map_err(SlotdError::from)?;
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
            print_squeue_jobs(&config, &jobs, &fields, args.noheader);
            Ok(())
        }
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response to squeue: {other:?}"
        ))),
    }
}

fn run_scontrol(config: AppConfig, args: ScontrolArgs) -> Result<()> {
    if !args.action.eq_ignore_ascii_case("show") || !args.entity.eq_ignore_ascii_case("job") {
        return Err(SlotdError::from(
            "supported syntax: scontrol show job <job_id>",
        ));
    }

    match send_request(
        &config,
        &Request::GetJob {
            job_id: args.job_id,
        },
    )? {
        Response::Job { job: Some(job) } => {
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
        Response::Job { job: None } => {
            Err(SlotdError::from(format!("job {} not found", args.job_id)))
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
            print_sacct_jobs(&config, &jobs, &fields, args.noheader);
            Ok(())
        }
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response to sacct: {other:?}"
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

fn run_sinfo(config: AppConfig, args: SinfoArgs) -> Result<()> {
    let fields = parse_sinfo_fields(args.format.as_deref()).map_err(SlotdError::from)?;
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
            Response::Job { job: Some(job) } if job.state.is_terminal() => return Ok(job),
            Response::Job { job: Some(_) } => thread::sleep(Duration::from_millis(200)),
            Response::Job { job: None } => {
                return Err(SlotdError::from(format!("job {job_id} disappeared")));
            }
            Response::Error { message } => return Err(SlotdError::from(message)),
            other => {
                return Err(SlotdError::from(format!(
                    "unexpected response while waiting for job {job_id}: {other:?}"
                )));
            }
        }
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
            Response::Job { job: Some(job) } if job.state == JobState::Running => return Ok(job),
            Response::Job { job: Some(job) } if job.state.is_terminal() => {
                return Err(SlotdError::from(format!(
                    "allocation {job_id} ended before it became runnable: {}",
                    job.state.as_str()
                )));
            }
            Response::Job { job: Some(_) } => thread::sleep(Duration::from_millis(200)),
            Response::Job { job: None } => {
                return Err(SlotdError::from(format!("allocation {job_id} disappeared")));
            }
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
    run_foreground_allocation_with_mode(config, job, command, false)
}

fn run_foreground_allocation_with_mode(
    config: &AppConfig,
    job: &JobRecord,
    command: &[String],
    record_step: bool,
) -> Result<()> {
    let step_record = if record_step {
        Some(start_step_record(config, job, command)?)
    } else {
        None
    };
    let mut child = Command::new(&command[0]);
    child.args(&command[1..]);
    child.current_dir(&job.cwd);
    child.stdin(Stdio::inherit());
    child.stdout(Stdio::inherit());
    child.stderr(Stdio::inherit());
    apply_slurm_env(&mut child, config, step_record.as_ref().unwrap_or(job));
    unsafe {
        child.pre_exec(|| {
            nix::unistd::setsid().map_err(std::io::Error::other)?;
            Ok(())
        });
    }
    let mut child = child.spawn()?;
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

fn replay_srun_output(job: &JobRecord, replay_stdout: bool, replay_stderr: bool) -> Result<()> {
    if replay_stdout {
        let stdout = fs::read_to_string(&job.stdout_path).unwrap_or_default();
        if !stdout.is_empty() {
            print!("{stdout}");
        }
    }
    if replay_stderr && job.stderr_path != job.stdout_path {
        let stderr = fs::read_to_string(&job.stderr_path).unwrap_or_default();
        if !stderr.is_empty() {
            eprint!("{stderr}");
        }
    }
    Ok(())
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
    immediate: bool,
    command: Vec<String>,
}

fn run_interactive_srun(config: &AppConfig, spec: InteractiveRunSpec) -> Result<()> {
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
        stdout_path: None,
        stderr_path: None,
    };

    let job_id = match send_request(
        config,
        &Request::SubmitAlloc {
            request,
            immediate: spec.immediate,
        },
    )? {
        Response::Submitted { job_id } => job_id,
        Response::Error { message } => return Err(SlotdError::from(message)),
        other => {
            return Err(SlotdError::from(format!(
                "unexpected response to interactive srun: {other:?}"
            )));
        }
    };

    let job = wait_for_job_running(config, job_id)?;
    run_foreground_allocation_with_mode(config, &job, &spec.command, true)
}

fn start_step_record(config: &AppConfig, parent: &JobRecord, command: &[String]) -> Result<JobRecord> {
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

    match send_request(config, &Request::GetJob { job_id: step_job_id })? {
        Response::Job { job: Some(job) } => Ok(job),
        Response::Job { job: None } => {
            Err(SlotdError::from(format!("step {step_job_id} disappeared")))
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
    command.env("SLURM_NTASKS", job.requested_tasks.to_string());
    command.env("SLURM_CPUS_PER_TASK", job.requested_cpus.to_string());
    if let Some(array_job_id) = job.array_job_id {
        command.env("SLURM_ARRAY_JOB_ID", array_job_id.to_string());
    }
    if let Some(array_task_id) = job.array_task_id {
        command.env("SLURM_ARRAY_TASK_ID", array_task_id.to_string());
    }
    command.env(
        "SLURM_STEP_ID",
        job.step_id.unwrap_or(0).to_string(),
    );
}

fn current_allocation_job(config: &AppConfig) -> Result<Option<JobRecord>> {
    let Ok(value) = std::env::var("SLURM_JOB_ID") else {
        return Ok(None);
    };
    let Ok(job_id) = value.parse::<i64>() else {
        return Ok(None);
    };

    match send_request(config, &Request::GetJob { job_id })? {
        Response::Job { job: Some(job) }
            if job.state == JobState::Running && job.allocation_only =>
        {
            Ok(Some(job))
        }
        Response::Job { .. } => Ok(None),
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response while loading current allocation: {other:?}"
        ))),
    }
}

fn run_foreground_step(config: &AppConfig, job: &JobRecord, command: &[String]) -> Result<()> {
    let step = start_step_record(config, job, command)?;
    let mut child = Command::new(&command[0]);
    child.args(&command[1..]);
    child.current_dir(&job.cwd);
    child.stdin(Stdio::inherit());
    child.stdout(Stdio::inherit());
    child.stderr(Stdio::inherit());
    apply_slurm_env(&mut child, config, &step);
    let mut child = child.spawn()?;
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
        "   NumTasks={} CPUs/Task={} ReqMem={}MB ReqGRES=gpu:{} TimeLimit={} Dependency={}",
        job.requested_tasks,
        job.requested_cpus,
        job.requested_memory_mb,
        job.requested_gpus,
        time_limit,
        dependency
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
        job.requested_memory_mb.saturating_mul(1024 * 1024).to_string(),
    )?;
    let requested_cpus = job.requested_cpus.saturating_mul(job.requested_tasks).max(1);
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

    use super::{dispatch_argv0, format_duration_secs, format_timestamp, parse_time_filter};

    #[test]
    fn argv0_dispatch_inserts_slurm_alias() {
        let argv = vec![OsString::from("squeue"), OsString::from("--noheader")];
        let dispatched = dispatch_argv0(argv);
        assert_eq!(dispatched[1], OsString::from("squeue"));
        assert_eq!(dispatched[2], OsString::from("--noheader"));
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
}
