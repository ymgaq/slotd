# slotd Command Reference

## Purpose

This document describes the current user-facing command surface and runtime behavior of `slotd`.
It is an implementation reference for the current codebase, not a roadmap.

## Scope

`slotd` is a Slurm-style scheduler for a single host.

- one local daemon
- one local SQLite database
- one execution node
- batch jobs, interactive runs, allocations, and steps
- reservation-based CPU, memory, and GPU scheduling
- optional cgroup v2 enforcement when `SLOTD_CGROUP_BASE` is set

It is not a multi-node Slurm controller.

## Binary and Aliases

The primary binary is `slotd`.

Available subcommands:

- `slotd daemon`
- `slotd sbatch`
- `slotd srun`
- `slotd salloc`
- `slotd squeue`
- `slotd sacct`
- `slotd scontrol`
- `slotd scancel`
- `slotd sinfo`

Alias execution through `argv[0]` is also supported:

- `sbatch`
- `srun`
- `salloc`
- `squeue`
- `sacct`
- `scontrol`
- `scancel`
- `sinfo`

## Runtime Layout

The default runtime root is `var/`.

Important paths:

- socket: `var/run/slotd.sock`
- database: `var/lib/state.db`
- jobs: `var/lib/jobs/<job_id>/`
- batch script: `var/lib/jobs/<job_id>/script.sh`
- daemon wrapper script: `var/lib/jobs/<job_id>/runner.sh`
- daemon exit status file: `var/lib/jobs/<job_id>/exit_status`

You can override the runtime root with `SLOTD_ROOT`. If `SLOTD_ROOT` is unset, `slotd` uses the relative `var/` path, so the daemon and the client must either share the same working directory or the same absolute `SLOTD_ROOT`.

## Environment Variables

### Partitions

- `SLOTD_CPU_PARTITIONS`
  - comma-separated CPU partition names
  - default: `cpu`
- `SLOTD_GPU_PARTITIONS`
  - comma-separated GPU partition names
  - default: `gpu` when GPUs are available

### Resources

- `SLOTD_GPU_COUNT`
  - number of GPU slots on the local host
  - if unset, `slotd` tries to detect GPUs via `nvidia-smi`
- `SLOTD_GPU_MODEL`
  - display name for the GPU model
  - if unset, `slotd` tries to detect it via `nvidia-smi`
- `SLOTD_FEATURES`
  - comma-separated feature names used by `--constraint`
  - `cpu` is always added implicitly, and `gpu` is added when GPUs are available

### Runtime Control

- `SLOTD_CGROUP_BASE`
  - base directory for cgroup v2 control
  - when set, `slotd` creates per-job cgroups and writes `memory.max` and `cpu.max`
- `SLOTD_NOTIFY_CMD`
  - notification hook executed through `/bin/sh -lc` when a terminal top-level job completes

### `sbatch` Environment Overrides

For `sbatch`, supported `SBATCH_*` environment variables take precedence over `#SBATCH` directives.
Supported items include:

- `SBATCH_JOB_NAME`
- `SBATCH_PARTITION`
- `SBATCH_CPUS_PER_TASK`
- `SBATCH_NTASKS`
- `SBATCH_MEM`
- `SBATCH_TIME`
- `SBATCH_GPUS`
- `SBATCH_CONSTRAINT`
- `SBATCH_BEGIN`
- `SBATCH_EXCLUSIVE`
- `SBATCH_REQUEUE`
- `SBATCH_OUTPUT`
- `SBATCH_ERROR`
- `SBATCH_CHDIR`
- `SBATCH_DEPENDENCY`
- `SBATCH_ARRAY_INX`
- `SBATCH_EXPORT`
- `SBATCH_EXPORT_FILE`
- `SBATCH_OPEN_MODE`

## Partitions and Defaults

Partition names are validated against the configured CPU and GPU partition lists.

Current default behavior:

- default partition: the first configured partition
- default CPU request: `1` task with `1` CPU per task
- default memory request: `512M`
- default GPU request: `1` on GPU partitions, otherwise `0`

## Supported Job States

Implemented states:

- `PENDING`
- `RUNNING`
- `COMPLETING`
- `COMPLETED`
- `FAILED`
- `CANCELLED`
- `TIMEOUT`
- `OUT_OF_MEMORY`

Terminal states:

- `COMPLETED`
- `FAILED`
- `CANCELLED`
- `TIMEOUT`
- `OUT_OF_MEMORY`

## Scheduling Model

The daemon scheduling loop runs every `300ms`.

Jobs may remain pending because of:

- unsatisfied dependencies
- array concurrency limits
- delayed start times
- exclusive host usage
- insufficient reserved resources
- user hold state

Ordering rules:

- submission order is the base rule
- explicit priority can override pure submission order
- array tasks are interleaved by array group

## `sbatch`

### Forms

```bash
sbatch [options] <script>
sbatch [options] --wrap '<command>'
```

### Main Options

- `--wrap <command>`
- `-J`, `--job-name <name>`
- `-p`, `--partition <partition>`
- `-c`, `--cpus-per-task <n>`
- `-n`, `--ntasks <n>`
- `--mem <size>`
- `-t`, `--time <time>`
- `-G`, `--gpus <n>`
- `-o`, `--output <path>`
- `-e`, `--error <path>`
- `-D`, `--chdir <path>`
- `--constraint <feature>`
- `-d`, `--dependency <spec>`
- `-a`, `--array <spec>`
- `--export <spec>`
- `--export-file <path>`
- `--open-mode append|truncate`
- `--signal <spec>`
- `--begin <time>`
- `--exclusive`
- `--requeue`
- `--parsable`
- `-W`, `--wait`

### `#SBATCH` Support

Supported directives:

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
- `--constraint`
- `--begin`
- `--exclusive`
- `--requeue`
- `-d`, `--dependency`
- `-a`, `--array`

Precedence:

1. command-line options
2. `SBATCH_*` environment variables
3. `#SBATCH` directives
4. built-in defaults

### Supported Dependency Expressions

- `after:<jobid>[,<jobid>...]`
- `afterany:<jobid>[,<jobid>...]`
- `afterok:<jobid>[,<jobid>...]`
- `afternotok:<jobid>[,<jobid>...]`
- `singleton`

### Supported Array Syntax

- single IDs
- ranges such as `0-7`
- stepped ranges such as `0-15:2`
- concurrency limits such as `0-31%4`

### Output Pattern Tokens

- `%j`: job ID
- `%A`: array job ID
- `%a`: array task ID
- `%x`: job name
- `%u`: user name
- `%N`: hostname
- `%%`: literal `%`

Defaults:

- non-array stdout: `slurm-%j.out`
- array stdout: `slurm-%A_%a.out`
- stderr defaults to stdout when `--error` is not set

## `srun`

### Form

```bash
srun [options] -- <command...>
```

### Behavior

By default, `srun` runs the command in the foreground.

- inside an allocation:
  - creates a step record
  - runs the command directly in the foreground
- outside an allocation:
  - creates an allocation-like top-level record
  - waits until resources are available
  - creates a step record
  - runs the command in the foreground

Only `--no-wait` submits a daemon-managed run job.

### Main Options

- `-J`, `--job-name <name>`
- `-p`, `--partition <partition>`
- `-c`, `--cpus-per-task <n>`
- `-n`, `--ntasks <n>`
- `--mem <size>`
- `-t`, `--time <time>`
- `-G`, `--gpus <n>`
- `-o`, `--output <path>`
- `-e`, `--error <path>`
- `-D`, `--chdir <path>`
- `--immediate`
- `--pty`
- `--constraint <feature>`
- `--cpu-bind <mode>`
- `--label`
- `--unbuffered`
- `--no-wait`

Supported CPU binding values:

- `none`
- `cores`
- `map_cpu:<id,id,...>`

## `salloc`

### Form

```bash
salloc [options] [command...]
```

### Behavior

`salloc` creates an allocation-only top-level job, waits until the allocation is runnable, and then starts a foreground command inside it.

If no command is given, it starts the current shell.

### Main Options

- `-J`, `--job-name <name>`
- `-p`, `--partition <partition>`
- `-c`, `--cpus-per-task <n>`
- `-n`, `--ntasks <n>`
- `--mem <size>`
- `-t`, `--time <time>`
- `-G`, `--gpus <n>`
- `-D`, `--chdir <path>`
- `--constraint <feature>`
- `--immediate`

## `squeue`

`squeue` shows queued and running top-level jobs.

Supported options:

- `--all`
- `-t`, `--states`
- `-j`, `--jobs`
- `-u`, `--user`
- `-p`, `--partition`
- `-o`, `--format`
- `-S`, `--sort`
- `-l`, `--long`
- `--start`
- `--array`
- `--noheader`

Default view:

```text
JOBID | PARTITION | NAME | USER | ST | TIME | NODELIST(REASON)
```

Long view:

```text
JOBID | PARTITION | NAME | USER | ST | TIME | TIME_LIMIT | NTASKS | CPUS | REQ_MEM | REQ_GPU | NODELIST(REASON)
```

## `sacct`

`sacct` shows persisted accounting data, including completed jobs and steps.

Supported options:

- `-j`, `--jobs`
- `-s`, `--state`
- `-S`, `--starttime`
- `-E`, `--endtime`
- `-u`, `--user`
- `-p`, `--partition`
- `-o`, `--format`
- `-P`, `--parsable2`
- `-n`, `--noheader`

Default view:

```text
JobID | Partition | JobName | User | State | ExitCode
```

Record types:

- top-level jobs
- allocation records
- step records
- completed records

ID rendering rules:

- step IDs appear as `<job_id>.<step_id>`
- array tasks appear as `<array_job_id>_<task_id>`

## `scontrol`

Supported forms:

```bash
scontrol show job <job_id>
scontrol hold job <job_id>
scontrol release job <job_id>
scontrol update job <job_id> KEY=VALUE...
```

Supported update keys:

- `JobName` / `Name`
- `Partition`
- `TimeLimit` / `Time`
- `Priority`

Rules:

- `JobName` and `Partition` can be changed only while the job is `PENDING`
- `TimeLimit` can be changed until the job reaches a terminal state
- `Priority` can be changed only while the job is `PENDING`

## `scancel`

Supported forms:

```bash
scancel <job_id>
scancel <job_id.step_id>
scancel --signal <sig> <job_id>
scancel --signal <sig> <job_id.step_id>
```

Default cancellation behavior:

- pending jobs become `CANCELLED` immediately
- running jobs transition through `COMPLETING`
- the runner sends `SIGTERM`
- after the grace period it sends `SIGKILL` if necessary

Recorded cancel reason:

```text
CancelledByUser
```

## `sinfo`

`sinfo` shows partition and host state for the local node.

Supported options:

- `-p`, `--partition`
- `-N`, `--Node`
- `-l`, `--long`
- `-o`, `--format`
- `--noheader`

Typical default output:

```text
PARTITION | HOSTNAMES | STATE | FEATURES | GRES_USED
cpu*      | localhost | idle  | cpu      | N/A
gpu       | localhost | idle  | cpu,gpu  | gpu:0
```

Long view adds:

- CPU capacity and allocated CPUs
- total and allocated memory
- total and allocated GPUs
- running and pending job counts

## Notifications

If `SLOTD_NOTIFY_CMD` is set, `slotd` executes it for terminal top-level jobs.

Exported variables:

- `SLOTD_JOB_ID`
- `SLOTD_JOB_NAME`
- `SLOTD_JOB_STATE`
- `SLOTD_JOB_PARTITION`
- `SLOTD_JOB_REASON`
