# slotd Implemented Specification

## Overview

This document summarizes the behavior that is implemented in the repository today.
It is intentionally narrower than [DESIGN.md](/home/yu_yamaguchi/workspace/slotd/DESIGN.md).
`DESIGN.md` describes the target architecture, while this file records the current MVP.

At this stage, `slotd` is a single-binary Rust application that provides:

- a local daemon
- batch job submission
- queue inspection
- job cancellation
- single-node resource display
- SQLite-backed job persistence

## Implemented Commands

The current binary supports these subcommands:

- `slotd daemon`
- `slotd sbatch <script>`
- `slotd squeue`
- `slotd scancel <job_id>`
- `slotd sinfo`

The CLI also supports Slurm-like command aliases through `argv[0]` dispatch for:

- `sbatch`
- `squeue`
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

## Implemented Job Lifecycle

The currently implemented job states are:

- `PENDING`
- `RUNNING`
- `COMPLETED`
- `FAILED`
- `CANCELLED`

State behavior:

- `sbatch` inserts a new job as `PENDING`
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
- resource admission is based on reserved CPU and memory values
- resources are derived from currently running jobs recorded in SQLite

Scheduler timing:

- the scheduler loop runs every `300ms`

Resource defaults:

- total CPUs: detected from `std::thread::available_parallelism()`
- total memory: fixed at `16384 MB`

## Resource Model

CPU and memory are currently used as scheduling reservations only.

Implemented behavior:

- `sbatch --cpus-per-task` sets requested CPUs
- `sbatch --mem` sets requested memory in MB
- `sinfo` reports total and allocated reserved resources
- jobs are admitted only if requested resources fit within remaining reserved capacity

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
- `--cpus-per-task`
- `--mem`
- `--output`
- `--error`

Supported `#SBATCH` directives in script contents:

- `--job-name`
- `--cpus-per-task`
- `--mem`
- `--output`
- `--error`

Precedence:

- explicit CLI options override `#SBATCH` directives
- `#SBATCH` directives override built-in defaults

Not implemented yet:

- partitions, accounts, priorities, dependencies, arrays

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
- state
- command string
- working directory
- requested CPUs
- requested memory
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
- list jobs
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

Current columns:

- `JOBID`
- `NAME`
- `ST`
- `CPU`
- `MEM`
- `SUBMIT_TIME`
- `COMMAND`

Notes:

- the state is shown using short codes such as `PD`, `R`, `CD`, `F`, `CA`
- timestamps are currently shown as raw Unix epoch seconds
- long strings are truncated to fixed widths

### `sinfo`

Current fields:

- total CPUs
- allocated CPUs
- total memory in MB
- allocated memory in MB
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

- `srun`
- structured config file
- `--json` output
- richer queue formatting
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
