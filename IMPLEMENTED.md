# slotd Implemented Specification

## Overview

This document summarizes the behavior implemented in the repository today.
It is intentionally narrower than [DESIGN.md](/home/yu_yamaguchi/workspace/slotd/DESIGN.md).
`DESIGN.md` describes target direction and roadmap, while this file records the
current code behavior.

At this stage, `slotd` is a single-binary Rust application that provides:

- a local daemon
- batch job submission
- foreground command execution via `srun`
- allocation-oriented foreground execution via `salloc`
- partition-aware scheduling for multiple configured CPU and GPU partition names
- queue inspection
- accounting inspection
- job cancellation
- single-node resource display
- SQLite-backed job persistence

## Implemented Commands

The current binary supports these subcommands:

- `slotd daemon`
- `slotd sbatch [options] <script>`
- `slotd sbatch --wrap <command>`
- `slotd srun [options] -- <command...>`
- `slotd salloc [options] [command...]`
- `slotd squeue`
- `slotd sacct`
- `slotd scancel <job_id>`
- `slotd sinfo`

The CLI also supports Slurm-like command aliases through `argv[0]` dispatch for:

- `sbatch`
- `srun`
- `salloc`
- `squeue`
- `sacct`
- `scancel`
- `sinfo`

## Runtime Layout

By default, runtime files are stored under `var/`.

Paths:

- socket: `var/run/slotd.sock`
- SQLite database: `var/lib/state.db`
- per-job directory: `var/lib/jobs/<job_id>/`
- job script: `var/lib/jobs/<job_id>/script.sh`

The root directory can be changed with the `SLOTD_ROOT` environment variable.

Partition names can be configured with:

- `SLOTD_CPU_PARTITIONS`
- `SLOTD_GPU_PARTITIONS`

GPU capacity and model can be configured with:

- `SLOTD_GPU_COUNT`
- `SLOTD_GPU_MODEL`

When available, `slotd` also attempts to detect GPU count and GPU model from
`nvidia-smi` during startup.

## Implemented Job Lifecycle

The currently implemented job states are:

- `PENDING`
- `RUNNING`
- `COMPLETING`
- `COMPLETED`
- `FAILED`
- `CANCELLED`
- `TIMEOUT`
- `OUT_OF_MEMORY`

State behavior:

- `sbatch` inserts a new job as `PENDING`
- `srun` inserts a command job and waits for completion by default
- `salloc` inserts an allocation-only job and waits until the allocation becomes runnable
- the daemon scheduler starts a pending job when enough reserved resources are available
- once spawned or reserved, the job becomes `RUNNING`
- cancellation and timeout requests pass through `COMPLETING`
- an exit code of `0` becomes `COMPLETED`
- a non-zero exit code or signal-based exit becomes `FAILED`
- `scancel` changes a pending job directly to `CANCELLED`
- jobs that exceed their configured time limit become `TIMEOUT`

Reason and termination tracking:

- jobs store a `state_reason`
- jobs store a terminating signal separately from numeric exit code
- queue and accounting output use these richer terminal details where available

## Scheduling Behavior

The current scheduler is FIFO and local-only.

Implemented behavior:

- the daemon checks for runnable jobs in ID order
- only one pending job is selected per scheduler loop iteration
- resource admission is based on reserved CPU, memory, and GPU values
- CPU reservation is `ntasks * cpus-per-task`
- resources are derived from currently running jobs recorded in SQLite
- both script-backed jobs and allocation-only jobs participate in the same scheduler

Scheduler timing:

- the scheduler loop runs every `300ms`

Resource defaults:

- total CPUs: detected from `std::thread::available_parallelism()`
- total memory: fixed at `16384 MB`
- total GPUs: derived from `SLOTD_GPU_COUNT` or `nvidia-smi`

## Resource Model

CPU and memory are currently used as scheduling reservations only.

Implemented behavior:

- `sbatch --cpus-per-task` sets requested CPUs per task
- `sbatch --ntasks` sets requested task count
- `sbatch --mem` sets requested memory in MB
- `sbatch --time` sets requested time limit
- `sbatch --partition` selects a configured partition
- `sbatch --gpus` sets requested GPU slots
- `srun --cpus-per-task` sets requested CPUs per task
- `srun --ntasks` sets requested task count
- `srun --mem` sets requested memory in MB
- `srun --time` sets requested time limit
- `srun --partition` selects a configured partition
- `srun --gpus` sets requested GPU slots
- `salloc` accepts the same resource flags as `srun` except output path flags
- `sinfo` reports partition state and GRES-style usage
- jobs are admitted only if requested resources fit within remaining reserved capacity

Partition behavior:

- CPU partitions come from `SLOTD_CPU_PARTITIONS`, default `cpu`
- GPU partitions come from `SLOTD_GPU_PARTITIONS`, default `gpu` when GPUs exist
- if no GPUs are available, no GPU partition is exposed
- if a partition is configured as a GPU partition and `--gpus` is omitted, the default GPU count is `1`
- when a GPU job starts, specific GPU IDs are assigned from the free pool
- assigned GPU IDs are exported through `CUDA_VISIBLE_DEVICES`

Not implemented yet:

- cgroup-based runtime enforcement
- CPU pinning
- actual memory limits

## Batch Submission

`sbatch` currently works as follows:

- reads the target script from disk, or accepts `--wrap`
- stores the script body in the job directory as `script.sh`
- records the submission working directory, or `--chdir` if provided
- stores the requested resource values
- derives the default job name from the input script file name
- prints `Submitted batch job <id>` by default
- prints just `<id>` when `--parsable` is used
- waits for completion and returns the job exit code when `--wait` is used

Supported CLI options:

- `--wrap`
- `-J`, `--job-name`
- `-p`, `--partition`
- `-c`, `--cpus-per-task`
- `-n`, `--ntasks`
- `--mem`
- `-t`, `--time`
- `-G`, `--gpus`
- `-o`, `--output`
- `-e`, `--error`
- `-D`, `--chdir`
- `--parsable`
- `-W`, `--wait`

Supported `#SBATCH` directives in script contents:

- `-J`, `--job-name`
- `-p`, `--partition`
- `-c`, `--cpus-per-task`
- `-n`, `--ntasks`
- `--mem`
- `-t`, `--time`
- `-G`, `--gpus`
- `-o`, `--output`
- `-e`, `--error`
- `-D`, `--chdir`

Directive parsing behavior:

- only `#SBATCH` lines in the initial comment block are interpreted
- once the first executable line is reached, later `#SBATCH` lines are ignored

Precedence:

- explicit CLI options override `#SBATCH` directives
- `#SBATCH` directives override built-in defaults

Currently implemented output path behavior:

- default stdout path is `slurm-%j.out`
- if `--error` is not specified, stderr is sent to the same file as stdout
- output patterns currently support `%j`, `%x`, `%u`, `%N`, and `%%`

Not implemented yet:

- dependencies
- arrays
- accounts
- priorities

## Command Submission

`srun` currently works as follows:

- accepts a direct command after `--`
- builds a small shell script wrapper internally
- submits the command as a scheduler-managed job
- records the command string in the job table for display in `squeue`
- waits for job completion by default
- returns the job exit code through the `slotd` process exit code
- replays captured output to the caller after completion when default log paths are used

Supported options:

- `-J`, `--job-name`
- `-p`, `--partition`
- `-c`, `--cpus-per-task`
- `-n`, `--ntasks`
- `--mem`
- `-t`, `--time`
- `-G`, `--gpus`
- `-o`, `--output`
- `-e`, `--error`
- `-D`, `--chdir`
- `--immediate`

Current `--immediate` behavior:

- if enough resources are available right now, the command job is accepted
- if enough resources are not available, submission fails immediately

Not implemented yet:

- interactive real-time stdio streaming
- `--pty`
- task and step semantics comparable to full Slurm `srun`

## Allocation Submission

`salloc` currently works as follows:

- accepts an optional command
- if no command is provided, runs the current shell from `$SHELL` or `/bin/bash`
- submits an allocation-only job
- waits until the allocation is runnable
- runs the command locally in the foreground with stdio inherited from the terminal
- exports basic Slurm-style environment variables before launching the command
- marks the allocation finished in the daemon when the foreground command exits

Supported options:

- `-J`, `--job-name`
- `-p`, `--partition`
- `-c`, `--cpus-per-task`
- `-n`, `--ntasks`
- `--mem`
- `-t`, `--time`
- `-G`, `--gpus`
- `-D`, `--chdir`
- `--immediate`

Current limitations:

- allocation cancellation depends on the CLI-adopted process group path
- there is no separate concept of nested job steps inside the allocation

## Job Execution

The daemon launches script-backed jobs directly with `/bin/bash`.

Implemented behavior:

- the stored script path is executed with `/bin/bash`
- the job runs in the recorded submission working directory
- stdin is closed for daemon-launched jobs
- stdout and stderr are redirected to configured output files
- if stdout and stderr resolve to the same path, one file is shared for both streams
- the child process is started in a dedicated session via `setsid()`
- the daemon tracks the child in memory while it is running
- the daemon enforces configured time limits and terminates overdue jobs
- basic Slurm-style environment variables are exported for daemon-launched jobs

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
- requested CPUs per task
- requested task count
- requested memory
- requested GPUs
- whether the job is allocation-only
- configured time limit
- assigned GPU IDs
- submit, start, and end timestamps
- PID and PGID
- exit code
- terminating signal
- state reason
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
- submit allocation-only job
- adopt allocation process metadata
- finish allocation-only job
- list jobs for queue display
- list jobs for accounting display
- get a single job by ID
- cancel a job
- query node info

Implemented response types:

- submitted job ID
- job list
- single job record
- cancelled job ID
- node info payload
- error message

## Output Behavior

### `squeue`

Current behavior:

- by default only active jobs are shown
- `--all` shows historical jobs as well
- `-t`, `--states` filters by requested states
- `-j`, `--jobs` filters by job ID
- `-u`, `--user` filters by user
- `-p`, `--partition` filters by partition
- `-o`, `--format` selects a supported field list
- `--noheader` removes the header line

Default columns:

- `JOBID`
- `PARTITION`
- `NAME`
- `USER`
- `ST`
- `TIME`
- `NODELIST(REASON)`

Supported `squeue --format` fields:

- `JobID`
- `Partition`
- `Name` / `JobName`
- `User`
- `State` / `ST`
- `Time` / `Elapsed`
- `Reason` / `NodeList(Reason)`

Notes:

- state uses short codes such as `PD`, `R`, `CG`, `CD`, `F`, `CA`, `TO`, `OOM`
- `NODELIST(REASON)` shows hostname for running and completed jobs, and a simple reason token for others

### `sacct`

Current behavior:

- used to inspect current and historical jobs
- `-j`, `--jobs` filters by job ID
- `-s`, `--state` filters by job state
- `-S`, `--starttime` filters by lower time bound
- `-E`, `--endtime` filters by upper time bound
- `-u`, `--user` filters by user
- `-p`, `--partition` filters by partition
- `-o`, `--format` selects a supported field list
- `-n`, `--noheader` removes the header line

Default columns:

- `JobID`
- `Partition`
- `JobName`
- `User`
- `State`
- `ExitCode`

Supported `sacct --format` fields:

- `JobID`
- `JobName`
- `Partition`
- `User`
- `State`
- `Reason`
- `ExitCode`
- `Elapsed`

Formatting notes:

- output columns are width-aligned for the human-readable default mode
- `ExitCode` is currently rendered as `<code>:<signal>`

### `sinfo`

Current behavior:

- reports current single-node partition state
- `-p`, `--partition` filters by partition
- `-o`, `--format` selects a supported field list
- `--noheader` removes the header line

Default columns:

- `PARTITION`
- `HOSTNAMES`
- `STATE`
- `GRES_USED`

Supported `sinfo --format` fields:

- `Partition`
- `Hostnames`
- `State`
- `GresUsed`

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

It is still a packaging stub. The application currently runs against local `var/`
paths by default rather than `/run/slotd` and `/var/lib/slotd`.

## Current Limitations

The following planned features are not implemented yet:

- real-time `srun` stdio streaming
- `srun --pty`
- exact runtime detection for `OUT_OF_MEMORY`
- full Slurm `--format` syntax and field coverage
- dependency handling
- array jobs
- structured config file
- `--json` output
- cgroup v2 resource enforcement
- multi-job fairness or priority scheduling
- systemd-managed installation flow

## Verified Behavior

The repository is currently verified by unit tests for:

- argv[0] command alias dispatch
- time-filter parsing for `sacct`
- time-limit parsing for `sbatch` / `srun`
- `#SBATCH` parsing rules
- supported `--format` field parsing for `squeue`, `sacct`, and `sinfo`
- output pattern expansion for `%j`, `%x`, `%u`, `%N`, and `%%`
