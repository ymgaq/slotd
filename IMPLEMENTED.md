# slotd Implemented Specification

## Overview

This document summarizes the behavior that is implemented in the repository today.
It is intentionally narrower than [DESIGN.md](/home/yu_yamaguchi/workspace/slotd/DESIGN.md).
`DESIGN.md` describes the target architecture, while this file records the current MVP.

At this stage, `slotd` is a single-binary Rust application that provides:

- a local daemon
- batch job submission
- command submission via `srun`
- partition-aware scheduling for `cpu` and `gpu`
- queue inspection
- job cancellation
- single-node resource display
- SQLite-backed job persistence

## Implemented Commands

The current binary supports these subcommands:

- `slotd daemon`
- `slotd sbatch <script>`
- `slotd srun [options] -- <command...>`
- `slotd squeue`
- `slotd sacct`
- `slotd scancel <job_id>`
- `slotd sinfo`

The CLI also supports Slurm-like command aliases through `argv[0]` dispatch for:

- `sbatch`
- `srun`
- `squeue`
- `sacct`
- `scancel`
- `sinfo`

This means a symlinked executable can behave like separate commands, although the
repository currently builds a single `slotd` binary.

## Runtime Layout

By default, runtime files are stored under `var/`.

Paths:

- socket: `var/run/slotd.sock`
- SQLite database: `var/lib/state.db`
- per-job directory: `var/lib/jobs/<job_id>/`
- job script: `var/lib/jobs/<job_id>/script.sh`
- stdout log: `var/lib/jobs/<job_id>/stdout.log`
- stderr log: `var/lib/jobs/<job_id>/stderr.log`

The root directory can be changed with the `SLOTD_ROOT` environment variable.

GPU capacity can be configured with:

- `SLOTD_GPU_COUNT`
- `SLOTD_GPU_MODEL`

When available, `slotd` also attempts to detect GPU count and GPU model from
`nvidia-smi` during startup.

## Implemented Job Lifecycle

The currently implemented job states are:

- `PENDING`
- `RUNNING`
- `COMPLETED`
- `FAILED`
- `CANCELLED`

State behavior:

- `sbatch` inserts a new job as `PENDING`
- `srun` can insert a command job and start it immediately when resources are free
- the daemon scheduler starts a pending job when enough reserved resources are available
- once spawned, the job becomes `RUNNING`
- an exit code of `0` becomes `COMPLETED`
- a non-zero exit code or signal-based exit becomes `FAILED`
- `scancel` changes a pending job directly to `CANCELLED`
- `scancel` sends signals to a running job and records `CANCELLED`

## Scheduling Behavior

The current scheduler is FIFO and local-only.

Implemented behavior:

- the daemon checks for runnable jobs in ID order
- only one pending job is selected per scheduler loop iteration
- resource admission is based on reserved CPU, memory, and GPU values
- resources are derived from currently running jobs recorded in SQLite
- supported partitions are `cpu` and `gpu`

Scheduler timing:

- the scheduler loop runs every `300ms`

Resource defaults:

- total CPUs: detected from `std::thread::available_parallelism()`
- total memory: fixed at `16384 MB`
- total GPUs: `SLOTD_GPU_COUNT`, default `1`

## Resource Model

CPU and memory are currently used as scheduling reservations only.

Implemented behavior:

- `sbatch --cpus-per-task` sets requested CPUs
- `sbatch --mem` sets requested memory in MB
- `sbatch --partition` selects `cpu` or `gpu`
- `sbatch --gpus` sets requested GPU slots
- `srun --cpus-per-task` sets requested CPUs
- `srun --mem` sets requested memory in MB
- `srun --partition` selects `cpu` or `gpu`
- `srun --gpus` sets requested GPU slots
- `sinfo` reports total and allocated reserved resources
- `sinfo` marks the default partition with `*`
- `sinfo` shows `N/A` for CPU-partition `GRES_USED`
- jobs are admitted only if requested resources fit within remaining reserved capacity

Partition behavior:

- `cpu` jobs must request `0` GPUs
- `gpu` jobs can request GPU slots
- `gpu` is the default partition
- if `gpu` is selected without an explicit GPU count, the default is `1`
- when a `gpu` job starts, specific GPU IDs are assigned from the free pool
- assigned GPU IDs are exported through `CUDA_VISIBLE_DEVICES`

Not implemented yet:

- cgroup-based runtime enforcement
- CPU pinning
- actual memory limits

## Batch Submission

`sbatch` currently works as follows:

- reads the target script from disk
- stores the script body in the job directory as `script.sh`
- records the current working directory as the job working directory
- stores the requested resource values
- derives the default job name from the input script file name

Supported CLI options:

- `--job-name`
- `--partition`
- `--cpus-per-task`
- `--mem`
- `--gpus`
- `--output`
- `--error`

Supported `#SBATCH` directives in script contents:

- `--job-name`
- `--partition`
- `--cpus-per-task`
- `--mem`
- `--gpus`
- `--output`
- `--error`

Precedence:

- explicit CLI options override `#SBATCH` directives
- `#SBATCH` directives override built-in defaults

Not implemented yet:

- partitions, accounts, priorities, dependencies, arrays

## Command Submission

`srun` currently works as follows:

- accepts a direct command after `--`
- builds a small shell script wrapper internally
- submits the command as a scheduler-managed job
- records the command string in the job table for display in `squeue`
- starts the job immediately if resources are currently available
- otherwise leaves it queued as `PENDING`

Supported options:

- `--job-name`
- `--partition`
- `--cpus-per-task`
- `--mem`
- `--gpus`
- `--output`
- `--error`
- `--immediate`

Current `--immediate` behavior:

- if enough resources are available right now, the command job is accepted
- if enough resources are not available, submission fails immediately

Not implemented yet:

- interactive stdio streaming back to the caller
- synchronous foreground waiting semantics like full Slurm `srun`

## Job Execution

The daemon launches jobs directly with `/bin/bash`.

Implemented behavior:

- the stored script path is executed with `/bin/bash`
- the job runs in the recorded submission working directory
- stdin is closed
- stdout and stderr are redirected to per-job log files by default
- `--output` and `--error` can override the default log destinations
- the child process is started in a dedicated session via `setsid()`
- the daemon tracks the child in memory while it is running

`scancel` behavior:

- pending jobs are cancelled in the database immediately
- running jobs receive `SIGTERM`
- after a `2s` grace period, still-running jobs receive `SIGKILL`

## Persistence

SQLite is the source of truth for job metadata.

The `jobs` table currently stores:

- job ID
- job name
- user name
- state
- partition
- command string
- working directory
- requested CPUs
- requested memory
- requested GPUs
- assigned GPU IDs
- submit, start, and end timestamps
- PID and PGID
- exit code
- script path
- stdout path
- stderr path

The schema is initialized from:

- [migrations/0001_init.sql](/home/yu_yamaguchi/workspace/slotd/migrations/0001_init.sql)

## IPC

The CLI and daemon communicate over a Unix domain socket using newline-delimited JSON.

Implemented request types:

- submit batch job
- submit command job
- list jobs for queue display
- list jobs for accounting display
- cancel job
- query node info

Implemented response types:

- submitted job ID
- job list
- cancelled job ID
- node info payload
- error message

## Output Behavior

### `squeue`

Current behavior:

- by default only active jobs are shown
- `--all` shows historical jobs as well
- `--states` filters by requested states

Current columns:

- `JOBID`
- `PARTITIO`
- `NAME`
- `USER`
- `ST`
- `TIME`
- `NODELIST(REASON)`

Notes:

- the default intent now matches Slurm more closely than the earlier implementation
- state uses short codes such as `PD`, `R`, `CD`, `F`, `CA`

### `sacct`

Current behavior:

- used to inspect current and historical jobs
- `-j/--jobs` filters by job ID
- `-s/--state` filters by job state

Current columns:

- `JobID`
- `Partition`
- `JobName`
- `User`
- `State`
- `ExitCode`

### `sinfo`

Current fields:

- partition name
- hostname
- partition state
- GRES-style GPU usage string
- total CPUs
- allocated CPUs
- total memory in MB
- allocated memory in MB
- total GPUs
- allocated GPUs
- running job count
- pending job count

## Recovery Behavior

Recovery is implemented in a minimal form.

Current behavior on daemon startup:

- `RUNNING` jobs with a live PID or PGID are adopted back into daemon tracking
- `RUNNING` jobs whose process is no longer alive are marked `FAILED`
- adopted jobs can still be cancelled after restart

Not implemented yet:

- exact exit-code recovery for jobs that survive a daemon restart
- preserving full child wait semantics across daemon restart

Current limitation:

- an adopted job that finishes after daemon restart is currently finalized as `FAILED`
  because the new daemon process cannot recover the original child exit status

## Packaging Files

The repository includes a systemd unit template at:

- [packaging/systemd/slotd.service](/home/yu_yamaguchi/workspace/slotd/packaging/systemd/slotd.service)

It is a packaging stub only. The application currently runs against local `var/`
paths by default rather than `/run/slotd` and `/var/lib/slotd`.

## Current Limitations

The following planned features are not implemented yet:

- full Slurm-like `squeue` and `sacct` option coverage
- structured config file
- `--json` output
- cgroup v2 resource enforcement
- multi-job fairness or priority scheduling
- systemd-managed installation flow

## Verified Behavior

The current MVP has been smoke-tested for the following flow:

1. start the daemon
2. submit a shell script with `sbatch`
3. observe the job in `squeue`
4. confirm execution output in `stdout.log`
5. inspect node usage with `sinfo`

This means the project is already at the stage of a working local prototype, not
just a scaffold.
