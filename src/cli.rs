use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use clap::{Args, Parser, Subcommand};

use crate::config::AppConfig;
use crate::daemon;
use crate::error::{Result, SlotdError};
use crate::ipc::{Request, Response, send_request};
use crate::job::{JobRecord, JobState, SubmitRequest};
use crate::output::{parse_sacct_fields, print_sacct_jobs, print_sinfo, print_squeue_jobs};
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
    #[arg(long)]
    mem: Option<String>,
    #[arg(long, short = 'G')]
    gpus: Option<u32>,
    #[arg(long, short = 'o')]
    output: Option<PathBuf>,
    #[arg(long, short = 'e')]
    error: Option<PathBuf>,
    #[arg(long, short = 'D')]
    chdir: Option<PathBuf>,
    #[arg(long)]
    parsable: bool,
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
    #[arg(long)]
    mem: Option<String>,
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
    #[arg(long, hide = true)]
    no_wait: bool,
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
    let requested_memory_mb = match &args.mem {
        Some(value) => parse_mem_mb(value)?,
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
        command_override,
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
            if args.parsable {
                println!("{job_id}");
            } else {
                println!("Submitted batch job {job_id}");
            }
            Ok(())
        }
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response to sbatch: {other:?}"
        ))),
    }
}

fn run_srun(config: AppConfig, args: SrunArgs) -> Result<()> {
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
        stdout_path: args.output.clone().map(|path| path.to_string_lossy().to_string()),
        stderr_path: args.error.clone().map(|path| path.to_string_lossy().to_string()),
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
            )))
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

fn run_squeue(config: AppConfig, args: SqueueArgs) -> Result<()> {
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
            print_squeue_jobs(&config, &jobs, args.noheader);
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
    let fields = parse_sacct_fields(args.format.as_deref()).map_err(SlotdError::from)?;
    let start_time = args
        .start_time
        .as_deref()
        .map(parse_time_filter)
        .transpose()?;
    let end_time = args.end_time.as_deref().map(parse_time_filter).transpose()?;

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
            print_sacct_jobs(&jobs, &fields, args.noheader);
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

fn run_sinfo(config: AppConfig) -> Result<()> {
    match send_request(&config, &Request::NodeInfo)? {
        Response::NodeInfo { info } => {
            print_sinfo(&config, &info);
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
                return Err(SlotdError::from(format!("job {job_id} disappeared")))
            }
            Response::Error { message } => return Err(SlotdError::from(message)),
            other => {
                return Err(SlotdError::from(format!(
                    "unexpected response while waiting for job {job_id}: {other:?}"
                )))
            }
        }
    }
}

fn replay_srun_output(job: &JobRecord, replay_stdout: bool, replay_stderr: bool) -> Result<()> {
    if replay_stdout {
        let stdout = fs::read_to_string(&job.stdout_path).unwrap_or_default();
        if !stdout.is_empty() {
            print!("{stdout}");
        }
    }
    if replay_stderr {
        let stderr = fs::read_to_string(&job.stderr_path).unwrap_or_default();
        if !stderr.is_empty() {
            eprint!("{stderr}");
        }
    }
    Ok(())
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
        "CD" | "COMPLETED" => Ok(JobState::Completed),
        "F" | "FAILED" => Ok(JobState::Failed),
        "CA" | "CANCELLED" => Ok(JobState::Cancelled),
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

    let days = days_from_civil(year, month, day)
        .ok_or_else(|| SlotdError::from("invalid date"))?;
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

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{dispatch_argv0, parse_time_filter};

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
}
