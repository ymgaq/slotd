# slotd Command Reference

## Purpose

This document describes the current `slotd` command surface and runtime behavior.
It is an implementation reference, not a roadmap.

Related documents:

- [DESIGN.md](/home/yu_yamaguchi/workspace/slotd/DESIGN.md): target direction and future phases
- [IMPLEMENTED.md](/home/yu_yamaguchi/workspace/slotd/IMPLEMENTED.md): compact summary of implemented features

## Scope

`slotd` is currently a single-node, Slurm-like scheduler with:

- one local daemon
- one local SQLite state database
- one local execution host
- batch jobs, interactive runs, allocations, and job steps
- CPU, memory, GPU reservation tracking
- optional cgroup v2 enforcement

It is not a multi-node Slurm controller.

## Binary And Command Aliases

The main binary is `slotd`.

Supported subcommands:

- `slotd daemon`
- `slotd sbatch`
- `slotd srun`
- `slotd salloc`
- `slotd squeue`
- `slotd sacct`
- `slotd scontrol`
- `slotd scancel`
- `slotd sinfo`

`argv[0]` alias dispatch is implemented, so the same binary can be invoked as:

- `sbatch`
- `srun`
- `salloc`
- `squeue`
- `sacct`
- `scontrol`
- `scancel`
- `sinfo`

## Runtime Layout

Default runtime root:

- `var/`

Runtime paths under the root:

- socket: `run/slotd.sock`
- database: `lib/state.db`
- jobs: `lib/jobs/<job_id>/`
- batch script: `lib/jobs/<job_id>/script.sh`
- daemon wrapper script: `lib/jobs/<job_id>/runner.sh`
- daemon exit status file: `lib/jobs/<job_id>/exit_status`

The root can be changed with:

- `SLOTD_ROOT`

## Configuration Environment Variables

### Partition Configuration

- `SLOTD_CPU_PARTITIONS`
  - comma-separated CPU partition names
  - default: `cpu`
- `SLOTD_GPU_PARTITIONS`
  - comma-separated GPU partition names
  - default: `gpu` when GPUs are available

### Resource Configuration

- `SLOTD_GPU_COUNT`
  - total GPU slots on the local host
  - if unset, `slotd` tries to infer this from `nvidia-smi`
- `SLOTD_GPU_MODEL`
  - display name used for GPU reporting
  - if unset, `slotd` tries to infer this from `nvidia-smi`

### Runtime Enforcement

- `SLOTD_CGROUP_BASE`
  - cgroup v2 base directory used for per-job cgroups
  - when set, launch paths try to create `slotd-<job_id>` directories below this base
  - foreground and daemon launch paths both use it
  - cgroup setup failures are treated as launch failures

## Resource Model

`slotd` tracks and schedules these resources:

- CPUs
- memory
- GPUs

Current behavior:

- CPU reservation is `ntasks * cpus-per-task`
- memory is stored in MB
- GPUs are stored as integer slots
- scheduling uses reserved capacity, not measured real-time usage
- the local host is the only execution node

Resource defaults:

- CPUs: `available_parallelism()`
- memory: `16384 MB`
- GPUs: `SLOTD_GPU_COUNT` or `nvidia-smi`

Partition behavior:

- only configured partition names are accepted
- if no GPU capacity exists, GPU partitions are not exposed
- if a GPU partition is selected and `--gpus` is omitted, the default GPU request is `1`
- otherwise the default GPU request is `0`
- optional host features come from `SLOTD_FEATURES`, plus implicit `cpu` and `gpu` markers

## Job Model

Each persisted record is one of:

- a regular batch job
- an allocation-only job
- an array task job
- a step job under an allocation

Important record fields:

- job id
- optional parent job id
- optional step id
- job name
- user name
- partition
- command
- working directory
- requested CPUs, tasks, memory, GPUs
- dependency string
- array metadata
- requeue flag and requeue count
- time limit
- pid, pgid
- exit code, terminating signal, state reason
- max RSS when observed

## Job States

Implemented states:

- `PENDING`
- `RUNNING`
- `COMPLETING`
- `COMPLETED`
- `FAILED`
- `CANCELLED`
- `TIMEOUT`
- `OUT_OF_MEMORY`

Short codes:

- `PD`
- `R`
- `CG`
- `CD`
- `F`
- `CA`
- `TO`
- `OOM`

Terminal states:

- `COMPLETED`
- `FAILED`
- `CANCELLED`
- `TIMEOUT`
- `OUT_OF_MEMORY`

## Scheduling Rules

The daemon loop runs every `300ms`.

Pending job admission checks:

- held jobs remain blocked with reason `JobHeldUser`
- unsatisfied dependencies block with reason `Dependency`
- array task concurrency limits block with reason `JobArrayTaskLimit`
- insufficient reserved resources block with reason `Resources`

Queue ordering:

- jobs are scheduled in FIFO order by submission time
- pending array tasks are interleaved by array group to reduce starvation

## Batch Script Parsing

`#SBATCH` directives are parsed only from the initial comment block.

Behavior:

- `#!` lines are ignored
- blank lines and comment lines are allowed before the first executable line
- once the first non-comment, non-empty line is seen, later `#SBATCH` lines are ignored
- command-line options override parsed `#SBATCH` values

Supported `#SBATCH` directives:

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

Environment precedence:

- command-line options override matching `SBATCH_*` environment variables
- matching `SBATCH_*` environment variables override `#SBATCH` directives
- `SBATCH_REQUEUE=1|true|yes` enables `sbatch` requeue by default

Phase 5 additions:

- `sbatch --requeue` requeues a top-level job once after `FAILED`, `TIMEOUT`, or `OUT_OF_MEMORY`
- `COMPLETED` and `CANCELLED` jobs do not auto-requeue
- `SLOTD_NOTIFY_CMD` registers a best-effort shell hook for terminal top-level job completion
- the hook receives `SLOTD_JOB_ID`, `SLOTD_JOB_NAME`, `SLOTD_JOB_STATE`, `SLOTD_JOB_PARTITION`, and `SLOTD_JOB_REASON`

## Output Path Expansion

Batch output patterns support:

- `%j`: job id
- `%A`: array job id
- `%a`: array task id, or `4294967294` when not in an array task
- `%x`: job name
- `%u`: user name
- `%N`: hostname
- `%%`: literal `%`

Default stdout path:

- non-array jobs: `slurm-%j.out`
- array jobs: `slurm-%A_%a.out`

Default stderr path:

- same file as stdout when `--error` is not specified

Relative output paths are resolved against the job working directory.

## Exported Environment Variables

Foreground execution paths export these variables to child processes:

- `SLURM_JOB_ID`
- `SLURM_JOB_NAME`
- `SLURM_JOB_PARTITION`
- `SLURM_JOB_NODELIST`
- `SLURM_SUBMIT_DIR`
- `SLURM_NTASKS`
- `SLURM_CPUS_PER_TASK`
- `SLURM_STEP_ID`

Array jobs also export:

- `SLURM_ARRAY_JOB_ID`
- `SLURM_ARRAY_TASK_ID`

GPU jobs also export:

- `CUDA_VISIBLE_DEVICES`

For a step under an allocation:

- `SLURM_JOB_ID` is the parent allocation id
- `SLURM_STEP_ID` is the persisted step id

## Command Reference

### `slotd daemon`

Starts the local scheduler daemon.

Behavior:

- creates runtime directories
- removes an existing socket file if present
- binds the UNIX socket
- opens SQLite state
- recovers adoptable running jobs
- polls running jobs, enforces timeouts, and schedules pending jobs

The daemon stays in the foreground.

### `slotd sbatch`

Submit a batch job or wrapped command.

Forms:

```bash
slotd sbatch [options] <script>
slotd sbatch [options] --wrap '<command>'
```

Supported options:

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
- `-d`, `--dependency <spec>`
- `-a`, `--array <spec>`
- `--begin <time>`
- `--exclusive`
- `--parsable`
- `-W`, `--wait`

Behavior:

- script mode reads the script from disk
- `--wrap` creates an internal shell script around the command
- the job is persisted as `PENDING`
- default partition is the configured default partition
- default resources are `cpus=1`, `ntasks=1`, `mem=512MB`
- default GPU request depends on the selected partition
- `--begin` supports epoch seconds, `YYYY-MM-DD`, `YYYY-MM-DDTHH:MM:SS`, and `now+<duration>`
- `--exclusive` prevents the job from sharing the single host with other running top-level jobs
- `--parsable` prints only the job id
- otherwise it prints `Submitted batch job <id>`
- `--wait` waits for completion and exits nonzero when any batch task fails
- for array jobs, `--wait` waits for all persisted tasks of the array root

Dependency syntax currently implemented:

- `after:<jobid>[,<jobid>...]`
- `afterany:<jobid>[,<jobid>...]`
- `afterok:<jobid>[,<jobid>...]`
- `afternotok:<jobid>[,<jobid>...]`
- `singleton`

Array syntax currently implemented:

- comma-separated task ids
- ranges: `0-7`
- stepped ranges: `0-15:2`
- concurrency limits: `0-31%4`

Notes:

- there is no separate umbrella array parent record; array tasks are persisted as normal job records and the first task id becomes the array root id

### `slotd srun`

Run a command in the foreground, or submit a daemon-managed command job depending on options.

Form:

```bash
slotd srun [options] -- <command...>
```

Supported options:

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

Execution modes:

- inside an active allocation:
  - `srun` creates a step record and runs the command directly in the foreground
- outside an allocation:
  - `srun` creates an allocation-like record, waits for it to run, then runs the command directly in the foreground
  - a step record is also created for accounting
  - `-o/-e` only change where foreground output is written
  - only `--no-wait` submits a daemon-managed command job

Behavior details:

- default resources are the same as `sbatch`
- default job name is the command basename
- `--immediate` fails if resources are not available immediately
- `--constraint` is checked against the local host feature set, not against remote nodes
- `--pty` currently selects the foreground execution path; it does not implement terminal allocation features beyond direct foreground execution
- `--cpu-bind` supports `none`, `cores`, and `map_cpu:<id,id,...>`
- `--label` prefixes foreground stdout/stderr lines with `0: `
- `--unbuffered` flushes foreground forwarded output eagerly
- daemon-managed `srun --no-wait` prints `Submitted run job <id>`
- nonzero exit codes are propagated back to the caller

### `slotd salloc`

Request an allocation and then run a foreground command inside it.

Form:

```bash
slotd salloc [options] [command...]
```

Supported options:

- `-J`, `--job-name <name>`
- `-p`, `--partition <partition>`
- `-c`, `--cpus-per-task <n>`
- `-n`, `--ntasks <n>`
- `--mem <size>`
- `-t`, `--time <time>`
- `-G`, `--gpus <n>`
- `-D`, `--chdir <path>`
- `--immediate`

Behavior:

- when no command is provided, the user shell is started
- the allocation record is created with `allocation_only = true`
- `salloc` prints `Granted job allocation <id>`
- it waits until the allocation becomes `RUNNING`
- then it runs the target command in the foreground with Slurm-like environment variables
- exit status is propagated to the caller

### `slotd squeue`

Show queue state for persisted jobs.

Supported options:

- `--all`
- `-t`, `--states <state1,state2,...>`
- `-j`, `--jobs <id1,id2,...>`
- `-u`, `--user <name>`
- `-p`, `--partition <name1,name2,...>`
- `-o`, `--format <spec>`
- `-S`, `--sort <spec>`
- `-l`, `--long`
- `--array`
- `--noheader`

Current behavior:

- default state filter is `PENDING,RUNNING`
- `--all` disables the default state filter
- steps are not shown in `squeue`; only top-level jobs are shown
- `--array` renders array task job ids as `<array_job_id>_<task_id>`

Default columns:

- `JOBID`
- `PARTITION`
- `NAME`
- `USER`
- `ST`
- `TIME`
- `NODELIST(REASON)`

Supported field names for `-o/--format`:

- `JobID`
- `Partition`
- `Name` or `JobName`
- `User`
- `ST` or `State`
- `Time` or `Elapsed`
- `NodeList(Reason)`, `NodeListReason`, `Reason`, `NodeList`

Supported `%` tokens for `-o/--format`:

- `%i`
- `%P`
- `%j`
- `%u`
- `%t`, `%T`
- `%M`
- `%R`, `%N`

Supported sort keys for `-S/--sort`:

- `i`, `jobid`
- `p`, `partition`
- `u`, `user`
- `t`, `state`
- `m`, `time`

A leading `-` reverses sort order.

### `slotd sacct`

Show persisted accounting data, including completed jobs and steps.

Supported options:

- `-j`, `--jobs <id1,id2,...>`
- `-s`, `--state <state1,state2,...>`
- `-S`, `--starttime <timestamp>`
- `-E`, `--endtime <timestamp>`
- `-u`, `--user <name>`
- `-p`, `--partition <name1,name2,...>`
- `-o`, `--format <spec>`
- `-P`, `--parsable2`
- `-n`, `--noheader`

Current behavior:

- includes both top-level jobs and steps
- step ids are rendered as `<job_id>.<step_id>`
- array tasks are rendered as `<array_job_id>_<task_id>`
- `ExitCode` is rendered as `<exit_code>:<signal>`
- `-P/--parsable2` uses `|` as a delimiter

Time filter formats currently accepted:

- `YYYY-MM-DD`
- `YYYY-MM-DDTHH:MM:SS`

Default columns:

- `JobID`
- `Partition`
- `JobName`
- `User`
- `State`
- `ExitCode`

Supported field names for `-o/--format`:

- `JobID`
- `ArrayJobID`
- `ArrayTaskID`
- `JobName`
- `Partition`
- `User`
- `State`
- `Reason`
- `ExitCode`
- `Elapsed`
- `AllocCPUS`
- `ReqMem`
- `ReqTRES`
- `AllocTRES`
- `NodeList`
- `Submit`
- `Start`
- `End`
- `WorkDir`
- `BatchFlag`
- `MaxRSS`

Supported `%` tokens for `-o/--format`:

- `%i`
- `%F`
- `%K`
- `%j`
- `%P`
- `%u`
- `%t`, `%T`
- `%R`
- `%X`
- `%M`
- `%C`
- `%m`
- `%b`
- `%B`
- `%N`
- `%V`
- `%S`
- `%E`
- `%Z`

### `slotd scontrol`

Supported forms:

```bash
slotd scontrol show job <job_id>
slotd scontrol hold job <job_id>
slotd scontrol release job <job_id>
slotd scontrol update job <job_id> KEY=VALUE...
```

Supported update keys:

- `JobName` or `Name`
- `Partition`
- `TimeLimit` or `Time`
- `Priority`

Current mutability rules:

- `JobName` can only be changed while `PENDING`
- `Partition` can only be changed while `PENDING`
- `TimeLimit` can be changed until the job reaches a terminal state
- `Priority` can only be changed while `PENDING`

`show job` output includes:

- identity and ownership
- state and reason
- requested resources
- time limit
- dependency string
- submit, start, end timestamps
- exit code
- array metadata
- batch flag
- working directory
- command
- stdout/stderr paths
- node list
- `ReqTRES`
- `AllocTRES`
- `MaxRSS`
- child step summary when steps exist

### `slotd scancel`

Cancel a top-level job or a recorded step.

Form:

```bash
slotd scancel <job_id>
slotd scancel <job_id.step_id>
```

Behavior:

- pending jobs are marked `CANCELLED` immediately
- running jobs transition through `COMPLETING`
- running jobs receive `SIGTERM`, then `SIGKILL` after the grace period
- cancellation reason is recorded as `CancelledByUser`
- if a cgroup OOM event is detected during termination, final state is `OUT_OF_MEMORY`

### `slotd sinfo`

Show partition and host state for the single local node.

Supported options:

- `-p`, `--partition <name1,name2,...>`
- `-N`, `--Node`
- `-l`, `--long`
- `-o`, `--format <spec>`
- `--noheader`

Current behavior:

- one row is shown per configured partition
- the default partition is marked with `*`
- `-N` and `-l` are accepted but currently do not change rendering

Default columns:

- `PARTITION`
- `HOSTNAMES`
- `STATE`
- `GRES_USED`

Supported field names for `-o/--format`:

- `Partition`
- `Hostnames`, `Hostname`, `NodeList`
- `State`
- `GresUsed`

Supported `%` tokens for `-o/--format`:

- `%P`
- `%N`
- `%t`, `%T`
- `%G`

## Step And Allocation Semantics

`slotd` distinguishes:

- top-level jobs
- allocation-only jobs
- steps recorded under an allocation

Important current behavior:

- `salloc` creates an allocation-only top-level job
- foreground `srun` outside an allocation creates an allocation-like top-level job and a step record
- `srun` inside an existing allocation creates only a step record
- `sacct` includes steps
- `squeue` excludes steps
- `scontrol show job <allocation_id>` includes a step summary
- `scancel <job.step>` resolves the step record and cancels that child record

## Runtime Enforcement And OOM Handling

When `SLOTD_CGROUP_BASE` is set:

- daemon-launched jobs create a per-job cgroup
- foreground allocations and steps create a per-job cgroup
- `memory.max` is written from requested memory
- `cpu.max` is written from requested CPU share
- the child pid is written to `cgroup.procs`

OOM handling:

- if cgroup memory events indicate OOM, final state becomes `OUT_OF_MEMORY`
- otherwise signal-based termination becomes `FAILED`

If cgroups are not configured:

- scheduling reservations still work
- enforcement is best-effort only through scheduler admission and timeout handling

## Recovery Behavior

The daemon persists enough metadata to recover many running jobs after a restart.

Implemented behavior:

- daemon-managed jobs write an `exit_status` file via a wrapper script
- on recovery, adopted jobs are checked for live process groups
- if the process group is gone, `slotd` tries to restore final state from `exit_status`
- if cgroup memory events indicate OOM, recovery prefers `OUT_OF_MEMORY`
- if no reliable terminal signal exists, recovery falls back to `FAILED` with reason `LostAfterRestart`

## Current Slurm Compatibility Boundaries

Implemented Slurm-like areas:

- familiar command names
- common submission flags
- `#SBATCH` parsing
- dependencies
- arrays
- `salloc` and allocation-local `srun`
- `scontrol show/hold/release/update job`
- `sacct`, `squeue`, `sinfo` custom formatting

Notable current differences from full Slurm:

- single-node only
- no real multi-node placement
- no accounts or QoS
- no `scontrol` support beyond `job`
- `squeue -l`, `sinfo -l`, and `sinfo -N` are parsed but not feature-complete
- `%` formatting support is a subset, not full Slurm coverage
- no separate array umbrella record
- `--pty` selects the foreground path but is not a full terminal management implementation
