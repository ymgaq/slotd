use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::app::config::AppConfig;
use crate::app::error::{Result, SlotdError};
use crate::command::helpers::validate_constraint;
use crate::submit::sbatch::{BatchDirectives, parse_mem_mb, parse_time_limit_secs};

#[cfg(test)]
pub(crate) const SUPPORTED_ROOT_COMMANDS: &[&str] = &[
    "daemon", "sbatch", "srun", "salloc", "scontrol", "squeue", "sacct", "scancel", "sinfo",
];
#[cfg(test)]
pub(crate) const SUPPORTED_USER_COMMANDS: &[&str] = &[
    "sbatch", "srun", "salloc", "scontrol", "squeue", "sacct", "scancel", "sinfo",
];
#[cfg(test)]
pub(crate) const CORE_RESOURCE_LONG_FLAGS: &[&str] = &[
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
    pub(crate) command: Commands,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
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
    pub(crate) job_name: Option<String>,
    #[arg(long, short = 'p')]
    pub(crate) partition: Option<String>,
    #[arg(long, short = 'c')]
    pub(crate) cpus_per_task: Option<u32>,
    #[arg(long, short = 'n')]
    pub(crate) ntasks: Option<u32>,
    #[arg(long)]
    pub(crate) mem: Option<String>,
    #[arg(long, short = 't')]
    pub(crate) time: Option<String>,
    #[arg(long, short = 'G')]
    pub(crate) gpus: Option<u32>,
    #[arg(long, short = 'D')]
    pub(crate) chdir: Option<PathBuf>,
    #[arg(long)]
    pub(crate) constraint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedResourceArgs {
    pub(crate) job_name: Option<String>,
    pub(crate) partition: String,
    pub(crate) cwd: String,
    pub(crate) requested_cpus: u32,
    pub(crate) requested_tasks: u32,
    pub(crate) requested_memory_mb: u64,
    pub(crate) requested_gpus: u32,
    pub(crate) time_limit_secs: Option<u64>,
    pub(crate) constraint: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct SbatchEnvOverrides {
    pub(crate) directives: BatchDirectives,
    pub(crate) export: Option<String>,
    pub(crate) export_file: Option<PathBuf>,
    pub(crate) open_mode: Option<String>,
    pub(crate) signal: Option<String>,
    pub(crate) begin: Option<String>,
    pub(crate) exclusive: bool,
    pub(crate) requeue: bool,
}

impl ResourceArgs {
    pub(crate) fn resolve(
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
    pub(crate) script: Option<PathBuf>,
    #[arg(long)]
    pub(crate) wrap: Option<String>,
    #[command(flatten)]
    pub(crate) resources: ResourceArgs,
    #[arg(long, short = 'o')]
    pub(crate) output: Option<PathBuf>,
    #[arg(long, short = 'e')]
    pub(crate) error: Option<PathBuf>,
    #[arg(long)]
    pub(crate) export: Option<String>,
    #[arg(long = "export-file")]
    pub(crate) export_file: Option<PathBuf>,
    #[arg(long = "open-mode")]
    pub(crate) open_mode: Option<String>,
    #[arg(long)]
    pub(crate) signal: Option<String>,
    #[arg(long)]
    pub(crate) begin: Option<String>,
    #[arg(long)]
    pub(crate) exclusive: bool,
    #[arg(long)]
    pub(crate) requeue: bool,
    #[arg(long, short = 'd')]
    pub(crate) dependency: Option<String>,
    #[arg(long, short = 'a')]
    pub(crate) array: Option<String>,
    #[arg(long)]
    pub(crate) parsable: bool,
    #[arg(long, short = 'W')]
    pub(crate) wait: bool,
}

#[derive(Debug, Args)]
pub struct ScontrolArgs {
    pub(crate) action: String,
    pub(crate) entity: String,
    pub(crate) job_id: i64,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) updates: Vec<String>,
}

#[derive(Debug, Args)]
pub struct ScancelArgs {
    #[arg(long, short = 's')]
    pub(crate) signal: Option<String>,
    pub(crate) job_id: String,
}

#[derive(Debug, Args)]
pub struct SqueueArgs {
    #[arg(long)]
    pub(crate) all: bool,
    #[arg(short = 't', long, value_delimiter = ',')]
    pub(crate) states: Option<Vec<String>>,
    #[arg(short = 'j', long = "jobs", value_delimiter = ',')]
    pub(crate) jobs: Option<Vec<i64>>,
    #[arg(short = 'u', long = "user")]
    pub(crate) user: Option<String>,
    #[arg(short = 'p', long = "partition", value_delimiter = ',')]
    pub(crate) partitions: Option<Vec<String>>,
    #[arg(short = 'o', long = "format")]
    pub(crate) format: Option<String>,
    #[arg(short = 'S', long = "sort")]
    pub(crate) sort: Option<String>,
    #[arg(short = 'l', long = "long")]
    pub(crate) long: bool,
    #[arg(long)]
    pub(crate) start: bool,
    #[arg(long)]
    pub(crate) array: bool,
    #[arg(long = "noheader")]
    pub(crate) noheader: bool,
}

#[derive(Debug, Args)]
pub struct SacctArgs {
    #[arg(short = 'j', long = "jobs", value_delimiter = ',')]
    pub(crate) jobs: Option<Vec<i64>>,
    #[arg(short = 's', long = "state", value_delimiter = ',')]
    pub(crate) states: Option<Vec<String>>,
    #[arg(short = 'S', long = "starttime")]
    pub(crate) start_time: Option<String>,
    #[arg(short = 'E', long = "endtime")]
    pub(crate) end_time: Option<String>,
    #[arg(short = 'u', long = "user")]
    pub(crate) user: Option<String>,
    #[arg(short = 'p', long = "partition", value_delimiter = ',')]
    pub(crate) partitions: Option<Vec<String>>,
    #[arg(short = 'o', long = "format")]
    pub(crate) format: Option<String>,
    #[arg(short = 'P', long = "parsable2")]
    pub(crate) parsable2: bool,
    #[arg(short = 'n', long = "noheader")]
    pub(crate) noheader: bool,
}

#[derive(Debug, Args)]
pub struct SrunArgs {
    #[command(flatten)]
    pub(crate) resources: ResourceArgs,
    #[arg(long, short = 'o')]
    pub(crate) output: Option<PathBuf>,
    #[arg(long, short = 'e')]
    pub(crate) error: Option<PathBuf>,
    #[arg(long)]
    pub(crate) immediate: bool,
    #[arg(long)]
    pub(crate) pty: bool,
    #[arg(long = "cpu-bind")]
    pub(crate) cpu_bind: Option<String>,
    #[arg(long)]
    pub(crate) label: bool,
    #[arg(long)]
    pub(crate) unbuffered: bool,
    #[arg(long, hide = true)]
    pub(crate) no_wait: bool,
    #[arg(required = true, num_args = 1.., trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) command: Vec<String>,
}

#[derive(Debug, Args)]
pub struct SallocArgs {
    #[command(flatten)]
    pub(crate) resources: ResourceArgs,
    #[arg(long)]
    pub(crate) immediate: bool,
    #[arg(num_args = 0.., trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) command: Vec<String>,
}

#[derive(Debug, Args)]
pub struct SinfoArgs {
    #[arg(short = 'p', long = "partition", value_delimiter = ',')]
    pub(crate) partitions: Option<Vec<String>>,
    #[arg(short = 'N', long = "Node")]
    pub(crate) node: bool,
    #[arg(short = 'l', long = "long")]
    pub(crate) long: bool,
    #[arg(short = 'o', long = "format")]
    pub(crate) format: Option<String>,
    #[arg(long = "noheader")]
    pub(crate) noheader: bool,
}

fn current_dir_string() -> String {
    std::env::current_dir()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string())
}
