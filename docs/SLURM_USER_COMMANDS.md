# Slurm User Command and Option Reference

This document summarizes the user-facing Slurm commands, major options, `#SBATCH` directives, execution concepts, job states, and commonly referenced environment variables based on the official Slurm documentation.
It does not discuss `slotd` compatibility or implementation status.

Important assumptions:

- Slurm behavior is heavily affected by site-local configuration
- an option may exist in the official manual but still be disabled or restricted on a specific cluster
- account, QoS, partition, constraint, GRES, GPU naming, accounting fields, mail features, and container integration are all site-dependent
- this document is a reference to the upstream command surface, not a guarantee of behavior on every cluster

Primary references:

- [Slurm Manual Index](https://slurm.schedmd.com/man_index.html)
- [Overview](https://slurm.schedmd.com/overview.html)
- [Quick Start User Guide](https://slurm.schedmd.com/quickstart.html)
- [CPU Management](https://slurm.schedmd.com/cpu_management.html)
- [Heterogeneous Jobs](https://slurm.schedmd.com/heterogeneous_jobs.html)
- [Job State Codes](https://slurm.schedmd.com/job_state_codes.html)
- [`sbatch`](https://slurm.schedmd.com/sbatch.html)
- [`srun`](https://slurm.schedmd.com/srun.html)
- [`salloc`](https://slurm.schedmd.com/salloc.html)
- [`squeue`](https://slurm.schedmd.com/squeue.html)
- [`scancel`](https://slurm.schedmd.com/scancel.html)
- [`sacct`](https://slurm.schedmd.com/sacct.html)
- [`sinfo`](https://slurm.schedmd.com/sinfo.html)
- [`scontrol`](https://slurm.schedmd.com/scontrol.html)
- [`sattach`](https://slurm.schedmd.com/sattach.html)
- [`sbcast`](https://slurm.schedmd.com/sbcast.html)
- [`sprio`](https://slurm.schedmd.com/sprio.html)
- [`sshare`](https://slurm.schedmd.com/sshare.html)
- [`sstat`](https://slurm.schedmd.com/sstat.html)
- [`scrontab`](https://slurm.schedmd.com/scrontab.html)

## Official Slurm Commands

The `Commands` section of the Slurm 25.11 manual index includes the following commands.

| Command | Official role | Typical user view |
| --- | --- | --- |
| `sacct` | Show job and step history from accounting data | essential for post-run inspection |
| `sacctmgr` | View and manage account data | mostly administrative |
| `salloc` | Obtain an allocation and release it after command completion | interactive resource reservation |
| `sattach` | Attach to a job step | connect to running step I/O |
| `sbatch` | Submit a batch script | primary batch submission command |
| `sbcast` | Broadcast files to allocated nodes | helper utility |
| `scancel` | Signal or cancel jobs and steps | stop and control work |
| `scontrol` | Show or modify Slurm state and configuration | detailed inspection and selected control actions |
| `scrontab` | Manage Slurm cron tables | scheduled recurring jobs |
| `scrun` | OCI runtime proxy for Slurm | mostly container integration |
| `sdiag` | Show scheduler diagnostics | administrative diagnostics |
| `sh5util` | Utility for `acct_gather_profile` data | helper utility |
| `sinfo` | Show node and partition information | resource inspection |
| `sprio` | Show priority breakdown | queue analysis |
| `squeue` | Show queued and running jobs | essential current-state view |
| `sreport` | Generate reports from accounting data | administrative reporting |
| `srun` | Launch parallel work | interactive launch and job-step execution |
| `sshare` | Show association shares | fairshare inspection |
| `sstat` | Show statistics for running jobs and steps | live runtime analysis |
| `strigger` | Set, get, or clear triggers | mostly administrative |
| `sview` | GUI frontend | graphical use |

Commands most users touch regularly:

- `sbatch`
- `srun`
- `salloc`
- `squeue`
- `scancel`
- `sacct`
- `sinfo`
- `scontrol`
- `sstat`
- `sprio`
- `sshare`

## Core Slurm Execution Model

Many Slurm misunderstandings come from mixing up jobs, allocations, and steps.

| Concept | Meaning |
| --- | --- |
| job | the unit Slurm tracks and schedules |
| allocation | the granted set of nodes, CPUs, memory, GRES, and similar resources |
| step | an execution unit launched inside an allocation, typically via `srun` |
| batch job | a script submitted through `sbatch` |
| interactive allocation | resources obtained ahead of time with `salloc` |
| array job | a family of related tasks defined by one job specification |
| heterogeneous job | a job composed of components with different resource requests |

Typical flow:

1. submit with `sbatch`, or request resources with `salloc`
2. inspect queue state with `squeue`
3. inspect running jobs with `sstat`
4. inspect completed jobs with `sacct`
5. cancel with `scancel` when necessary
6. use `scontrol show job <jobid>`, `sprio`, or `sshare` for deeper inspection

## `sbatch`, `#SBATCH`, and Environment Variable Precedence

`sbatch` has three configuration layers:

1. command-line options
2. environment variables
3. `#SBATCH` directives inside the script

Officially, precedence is:

1. command-line options
2. environment variables
3. `#SBATCH` directives

This means a variable such as `SBATCH_PARTITION=gpu` in the shell can override a partition requested inside the script.

## `#SBATCH` Rules

`#SBATCH` allows `sbatch` options to be written near the top of a batch script.

Important rules:

- only lines starting with `#SBATCH` are interpreted
- parsing applies only to the initial shebang / comment / blank-line region
- once the parser reaches the first non-comment, non-blank line, later `#SBATCH` lines are ignored
- `#SBATCH` lines are not shell syntax, so shell expansion and command substitution do not apply
- command-line arguments override equivalent `#SBATCH` directives

## Resource Request Model

The most common resources users express are:

- job name
- partition
- number of nodes
- number of tasks
- CPUs per task
- memory
- time limit
- GPUs or GRES
- constraints and node selection
- output and error paths
- dependencies
- arrays

The three main launch commands cover different flows:

- `sbatch`: submit a batch script for later execution
- `salloc`: acquire an allocation first, then run commands interactively
- `srun`: launch work directly, or launch a step inside an existing allocation

## Common `sbatch` Options

Representative options commonly used by end users:

- `-J`, `--job-name`
- `-p`, `--partition`
- `-A`, `--account`
- `-N`, `--nodes`
- `-n`, `--ntasks`
- `-c`, `--cpus-per-task`
- `--mem`
- `--mem-per-cpu`
- `-t`, `--time`
- `--time-min`
- `-o`, `--output`
- `-e`, `--error`
- `-D`, `--chdir`
- `-a`, `--array`
- `-d`, `--dependency`
- `--constraint`
- `--nodelist`
- `--exclude`
- `--exclusive`
- `--oversubscribe`
- `--begin`
- `--deadline`
- `--signal`
- `--requeue`
- `--mail-type`
- `--mail-user`
- `--export`
- `--export-file`
- `--gres`
- `--gpus`
- `--gpus-per-node`
- `--gpus-per-task`
- `--gpu-bind`
- `--qos`
- `--reservation`
- `--comment`
- `--parsable`
- `--wrap`

## Common `srun` Options

Representative `srun` options:

- `-J`, `--job-name`
- `-p`, `--partition`
- `-N`, `--nodes`
- `-n`, `--ntasks`
- `-c`, `--cpus-per-task`
- `--mem`
- `-t`, `--time`
- `-D`, `--chdir`
- `-o`, `--output`
- `-e`, `--error`
- `--constraint`
- `--exclusive`
- `--oversubscribe`
- `--immediate`
- `--pty`
- `--label`
- `--unbuffered`
- `--cpu-bind`
- `--mem-bind`
- `--gpu-bind`
- `--kill-on-bad-exit`
- `--mpi`
- `--multi-prog`
- `--gres`
- `--gpus`

## Common `salloc` Options

Representative `salloc` options:

- `-J`, `--job-name`
- `-p`, `--partition`
- `-A`, `--account`
- `-N`, `--nodes`
- `-n`, `--ntasks`
- `-c`, `--cpus-per-task`
- `--mem`
- `-t`, `--time`
- `-D`, `--chdir`
- `--constraint`
- `--exclusive`
- `--oversubscribe`
- `--immediate`
- `--gres`
- `--gpus`

## Queue, Accounting, and Inspection Commands

### `squeue`

Used to inspect queued and running jobs.

Common options:

- `-j`, `--jobs`
- `-u`, `--user`
- `-p`, `--partition`
- `-t`, `--states`
- `-o`, `--format`
- `-S`, `--sort`
- `-l`, `--long`
- `--start`
- `--array`
- `--noheader`

### `sacct`

Used to inspect accounting history.

Common options:

- `-j`, `--jobs`
- `-s`, `--state`
- `-S`, `--starttime`
- `-E`, `--endtime`
- `-u`, `--user`
- `-o`, `--format`
- `-P`, `--parsable2`
- `-n`, `--noheader`

### `sinfo`

Used to inspect nodes and partitions.

Common options:

- `-p`, `--partition`
- `-N`, `--Node`
- `-l`, `--long`
- `-o`, `--format`
- `--noheader`
- `--states`
- `--responding`
- `--list-reasons`

### `scontrol`

Users most often rely on:

- `scontrol show job <jobid>`
- `scontrol show node <node>`
- `scontrol show partition <partition>`
- `scontrol hold <jobid>`
- `scontrol release <jobid>`
- `scontrol update JobId=<id> ...`

### `scancel`

Typical forms:

- `scancel <jobid>`
- `scancel <jobid.stepid>`
- `scancel --signal <sig> <jobid>`
- filtered cancellation by user, partition, state, and similar selectors

### `sstat`

Used to inspect running jobs and steps.

Typical fields include:

- elapsed time
- CPU usage
- memory usage
- RSS
- disk I/O
- task-level statistics

### `sprio`

Used to break down priority contributions.

Typical components include:

- age
- fairshare
- job size
- partition factor
- QoS factor
- site factor

### `sshare`

Used to inspect association shares and fairshare state.

## Job State Codes

Common Slurm job states include:

- `PENDING`
- `RUNNING`
- `COMPLETING`
- `COMPLETED`
- `FAILED`
- `CANCELLED`
- `TIMEOUT`
- `OUT_OF_MEMORY`
- `NODE_FAIL`
- `PREEMPTED`
- `BOOT_FAIL`
- `DEADLINE`
- `SUSPENDED`
- `CONFIGURING`

See the official [Job State Codes](https://slurm.schedmd.com/job_state_codes.html) page for the full list and site-specific behavior notes.

## Common Environment Variables

Frequently referenced Slurm environment variables include:

- `SLURM_JOB_ID`
- `SLURM_JOB_NAME`
- `SLURM_JOB_NODELIST`
- `SLURM_JOB_PARTITION`
- `SLURM_SUBMIT_DIR`
- `SLURM_SUBMIT_HOST`
- `SLURM_NTASKS`
- `SLURM_CPUS_PER_TASK`
- `SLURM_MEM_PER_CPU`
- `SLURM_MEM_PER_NODE`
- `SLURM_GPUS`
- `SLURM_GPUS_PER_TASK`
- `SLURM_ARRAY_JOB_ID`
- `SLURM_ARRAY_TASK_ID`
- `SLURM_ARRAY_TASK_COUNT`
- `SLURM_STEP_ID`
- `SLURM_PROCID`
- `SLURM_LOCALID`
- `SLURM_NODEID`

The exact set depends on whether the command is a batch job, an interactive allocation, a step, an array task, or a cluster with extra plugins enabled.

## Practical Guidance

For day-to-day use, the smallest useful command set is usually:

- `sbatch`
- `srun`
- `salloc`
- `squeue`
- `scancel`
- `sacct`
- `sinfo`
- `scontrol show job`

When behavior differs from what you expect:

- check the site policy first
- inspect the effective request with `scontrol show job`
- inspect runtime statistics with `sstat`
- inspect finished records with `sacct`
- inspect queue position and reason fields with `squeue`
