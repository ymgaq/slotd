# slotd

`slotd` is a Rust-built single-node, single-user Slurm-style job scheduler for a personal workstation.

It keeps the familiar Slurm command names and many common flags, but runs everything on one local machine with a small Rust codebase and:

- one daemon
- one SQLite database
- one execution host

Start here: [Installation](#installation)

Current user-facing commands:

- `sbatch`
- `srun`
- `salloc`
- `squeue`
- `sacct`
- `scontrol`
- `scancel`
- `sinfo`

Online documentation is [Here](https://ymgaq.github.io/slotd/).
([日本語版](https://ymgaq.github.io/slotd/ja/), [中文版](https://ymgaq.github.io/slotd/cn/))

## What It Is

`slotd` is designed for local batch and interactive workloads such as:

- long-running experiments
- GPU jobs on a single workstation
- local resource reservation
- queueing work without a full Slurm cluster

It is not a multi-node scheduler, and it does not implement account/QoS/fairshare/federation features from full Slurm.

The implementation is intentionally Rust-first:

- one compiled Rust binary
- no Python runtime dependency
- SQLite for local persistent state
- direct process and signal handling from native code

## Features

- Slurm-style command aliases through `argv[0]`
- local daemon and Unix socket IPC
- SQLite-backed durable job state
- CPU, memory, and GPU reservation-based scheduling
- host-detected CPU and memory capacity with GPU autodetection
- true single-node multi-task execution for `--ntasks`
- optional cgroup v2 CPU/memory enforcement when `SLOTD_CGROUP_BASE` is set
- batch jobs, interactive runs, allocations, and steps
- dependencies and job arrays
- `--constraint`, `--begin`, `--exclusive`, `--requeue`
- `sbatch --export`, `--export-file`, `--open-mode`, `--signal`
- `srun --cpu-bind`, `--label`, `--unbuffered`
- `squeue --start`, `squeue --array`
- `sinfo -l`
- lightweight completion hooks with `SLOTD_NOTIFY_CMD`

## Requirements

- Linux or WSL
- Rust toolchain with `cargo`
- `systemd --user` if you want managed background startup
- `nvidia-smi` if you want automatic GPU detection

## Installation

### Clone the repository

```bash
git clone https://github.com/ymgaq/slotd.git
cd slotd
```

### One-command install

The repository includes a Rust-oriented installer that builds the project and installs the resulting binary:

```bash
./scripts/install.sh
```

By default it will:

- build `slotd` in `release` mode
- install binaries under `~/.local/bin`
- create Slurm-style aliases such as `sbatch` and `squeue`
- create a runtime root under `~/.local/share/slotd`
- write configuration to `~/.config/slotd/slotd.env`
- install and start a `systemd --user` service

### Installer options

| Option | Description | Default |
| --- | --- | --- |
| `--repo-root PATH` | Build from a different repository root | current repo |
| `--profile NAME` | Cargo profile to build | `release` |
| `--install-bin-dir PATH` | Install location for `slotd` and aliases | `~/.local/bin` |
| `--runtime-root PATH` | Runtime root used as `SLOTD_ROOT` | `~/.local/share/slotd` |
| `--config-dir PATH` | Configuration directory for `slotd.env` | `~/.config/slotd` |
| `--systemd-user-dir PATH` | `systemd --user` unit directory | `~/.config/systemd/user` |
| `--cpu-partitions VALUE` | Value for `SLOTD_CPU_PARTITIONS` | `cpu` |
| `--gpu-partitions VALUE` | Value for `SLOTD_GPU_PARTITIONS` | `gpu` |
| `--features VALUE` | Value for `SLOTD_FEATURES` | unset |
| `--notify-cmd VALUE` | Value for `SLOTD_NOTIFY_CMD` | unset |
| `--cgroup-base PATH` | Value for `SLOTD_CGROUP_BASE` | unset |
| `--skip-build` | Reuse an existing cargo build output | off |
| `--skip-systemd` | Do not install or start a user service | off |
| `--uninstall` | Remove the installed setup | off |
| `--purge-runtime` | With `--uninstall`, also remove persisted state | off |

Example:

```bash
./scripts/install.sh \
  --runtime-root "$HOME/.local/share/slotd" \
  --cpu-partitions cpu \
  --gpu-partitions gpu \
  --features cpu,gpu \
  --notify-cmd 'notify-send "slotd" "$SLOTD_JOB_ID $SLOTD_JOB_STATE"'
```

If `--cgroup-base` is left unset, CPU and memory remain reservation-only. If it
is set, it must point at a writable cgroup v2 subtree or job launch fails
clearly.

### Uninstall

Remove binaries, aliases, config, and the user service:

```bash
./scripts/install.sh --uninstall
```

Also remove persisted jobs and runtime state:

```bash
./scripts/install.sh --uninstall --purge-runtime
```

## Quick Start

If you installed through `scripts/install.sh`, the daemon should already be running under `systemd --user`.

Basic checks:

```bash
sinfo
squeue
sacct
```

Typical output:

- `sinfo` shows one row per configured partition, for example `cpu` and `gpu`
- `squeue` is usually empty immediately after a fresh install
- `sacct` is usually empty until you submit jobs

Submit a simple batch job:

```bash
sbatch --wrap 'echo hello from slotd'
```

Typical output:

```text
Submitted batch job 1
```

Watch the queue:

```bash
squeue
```

Typical output while the job is waiting or running:

```text
JOBID | PARTITION | NAME | USER | ST | TIME | NODELIST(REASON)
1     | cpu       | wrap | ...  | R  | 0:00 | localhost
```

See completed jobs:

```bash
sacct
```

Typical output after completion:

```text
JobID | Partition | JobName | User | State     | ExitCode
1     | cpu       | wrap    | ...  | COMPLETED | 0:0
```

Show detailed job info:

```bash
scontrol show job 1
```

Typical output:

- job identity such as `JobId=1` and `JobName=wrap`
- current or final state such as `JobState=COMPLETED`
- requested resources, working directory, command, and output paths

## Common Workflows

### 1. Submit a simple CPU batch job

```bash
sbatch \
  -J hello \
  -p cpu \
  -c 1 \
  --mem 512M \
  -t 00:05:00 \
  -o logs/%j.out \
  --wrap 'echo hello'
```

Typical output:

```text
Submitted batch job 2
```

Expected result:

- `logs/2.out` is created
- the file contains `hello`

### 2. Submit a GPU batch job

```bash
sbatch \
  -J gpu-demo \
  -p gpu \
  -c 4 \
  --mem 8G \
  -G 1 \
  -t 01:00:00 \
  -o logs/%j.out \
  --wrap 'nvidia-smi'
```

Typical output:

```text
Submitted batch job 3
```

Expected result:

- the job is scheduled on the `gpu` partition
- `logs/3.out` contains `nvidia-smi` output

### 3. Submit a batch script with `#SBATCH` directives

Create a batch script:

```bash
cat > /tmp/slotd-demo.sh <<'EOF'
#!/usr/bin/env bash
#SBATCH -J script-demo
#SBATCH -p cpu
#SBATCH -c 2
#SBATCH --mem 1G
#SBATCH -t 00:05:00
#SBATCH -o logs/%j.out

echo "hello from script mode"
echo "job=$SLURM_JOB_ID cpus=$SLURM_CPUS_PER_TASK"
EOF
```

Submit it:

```bash
sbatch /tmp/slotd-demo.sh
```

Typical output:

```text
Submitted batch job 4
```

Expected result:

- the script header is parsed for resource settings such as job name, partition, CPUs, memory, and output path
- `logs/4.out` contains the echoed lines from the script body

### 4. Run an interactive command with `srun`

```bash
srun \
  -p cpu \
  -c 2 \
  --mem 1G \
  --label \
  --unbuffered \
  -- echo hello
```

Typical output:

```text
0: hello
```

### 5. Start an interactive allocation

```bash
salloc \
  -p gpu \
  -c 4 \
  --mem 8G \
  -G 1 \
  -t 00:30:00
```

Typical output:

```text
Granted job allocation 4
```

Expected result:

- your shell starts inside the allocation
- follow-up `srun` commands run as steps under that allocation

### 6. Submit an array job

```bash
sbatch \
  -J array-demo \
  -a 0-9%2 \
  -o logs/%A_%a.out \
  --wrap 'echo task=$SLURM_ARRAY_TASK_ID'
```

Typical output:

```text
Submitted batch job 5
```

Expected result:

- multiple task records are created
- files such as `logs/5_0.out`, `logs/5_1.out`, and so on are written

### 7. Requeue once on failure

```bash
sbatch \
  -J flaky \
  --requeue \
  --wrap 'exit 1'
```

Typical output:

```text
Submitted batch job 6
```

Expected result:

- the first failed run returns to `PENDING`
- after the second failure, `sacct` shows the final state as `FAILED`

### 8. Delay job start

```bash
sbatch \
  -J later \
  --begin now+00:10:00 \
  --wrap 'echo delayed'
```

Typical output:

```text
Submitted batch job 7
```

Expected result:

- `squeue` shows the job in `PENDING`
- `squeue --start` shows an estimated future start time

### 9. Pass custom environment variables

```bash
sbatch \
  --export FOO=bar,HELLO=world \
  --wrap 'echo "$FOO $HELLO"'
```

Typical output:

```text
Submitted batch job 8
```

Expected result:

- the job output contains `bar world`

## Command Overview

| Command | Purpose |
| --- | --- |
| `slotd daemon` | Start the local scheduler daemon |
| `sbatch` | Submit a batch job or wrapped command |
| `srun` | Run a foreground command, or submit a daemon-managed run with `--no-wait` |
| `salloc` | Request an allocation, then run a command inside it |
| `squeue` | Show queued and running top-level jobs |
| `sacct` | Show accounting data, including completed jobs and steps |
| `scontrol` | Show, hold, release, or update a job |
| `scancel` | Cancel a job or send a signal |
| `sinfo` | Show local partition and resource state |

## Important Options

### `sbatch`

| Option | Meaning |
| --- | --- |
| `--wrap <command>` | Submit an inline shell command instead of a script file |
| `-J`, `--job-name` | Set the job name |
| `-p`, `--partition` | Choose a configured partition |
| `-c`, `--cpus-per-task` | CPUs per task |
| `-n`, `--ntasks` | Number of concurrently launched local tasks |
| `--mem` | Requested memory, such as `512M` or `8G` |
| `-t`, `--time` | Time limit |
| `-G`, `--gpus` | Requested GPU slots |
| `-o`, `--output` | Stdout path pattern |
| `-e`, `--error` | Stderr path pattern |
| `-D`, `--chdir` | Working directory |
| `--constraint` | Require local features such as `cpu` or `gpu` |
| `-d`, `--dependency` | Dependency expression |
| `-a`, `--array` | Array specification |
| `--export` | Export environment values into the job |
| `--export-file` | Load environment variables from a file |
| `--open-mode append\|truncate` | Control output file append/truncate behavior |
| `--signal` | Configure a warning signal before the time limit |
| `--begin` | Delay job eligibility |
| `--exclusive` | Do not share the host with other top-level jobs |
| `--requeue` | Requeue once after `FAILED`, `TIMEOUT`, or `OUT_OF_MEMORY` |
| `--parsable` | Print only the job ID |
| `-W`, `--wait` | Wait for completion |

### `srun`

| Option | Meaning |
| --- | --- |
| `-J`, `--job-name` | Set the job name |
| `-p`, `--partition` | Choose a partition |
| `-c`, `--cpus-per-task` | CPUs per task |
| `-n`, `--ntasks` | Number of concurrently launched local tasks |
| `--mem` | Requested memory |
| `-t`, `--time` | Time limit |
| `-G`, `--gpus` | Requested GPU slots |
| `-o`, `--output` | Foreground stdout path |
| `-e`, `--error` | Foreground stderr path |
| `-D`, `--chdir` | Working directory |
| `--immediate` | Fail if resources are not available immediately |
| `--pty` | Reserved for PTY support; currently rejected with a clear error |
| `--constraint` | Require matching local features |
| `--cpu-bind` | CPU binding mode: `none`, `cores`, `map_cpu:<ids>` |
| `--label` | Prefix output lines with `<task_id>: ` |
| `--unbuffered` | Flush forwarded output eagerly |
| `--no-wait` | Submit a daemon-managed run job instead of waiting |

### `salloc`

| Option | Meaning |
| --- | --- |
| `-J`, `--job-name` | Set the allocation name |
| `-p`, `--partition` | Choose a partition |
| `-c`, `--cpus-per-task` | CPUs per task |
| `-n`, `--ntasks` | Number of concurrently launched local tasks |
| `--mem` | Requested memory |
| `-t`, `--time` | Time limit |
| `-G`, `--gpus` | Requested GPU slots |
| `-D`, `--chdir` | Working directory |
| `--constraint` | Require matching local features |
| `--immediate` | Fail if the allocation cannot start immediately |

### `squeue`

| Option | Meaning |
| --- | --- |
| `--all` | Show all job states instead of only `PENDING` and `RUNNING` |
| `-t`, `--states` | Filter by state |
| `-j`, `--jobs` | Filter by job IDs |
| `-u`, `--user` | Filter by user |
| `-p`, `--partition` | Filter by partition |
| `-o`, `--format` | Choose output fields |
| `-S`, `--sort` | Sort rows |
| `-l`, `--long` | Use the long default view |
| `--start` | Show estimated start times |
| `--array` | Show array-style job IDs |
| `--noheader` | Omit the table header |

### `sacct`

| Option | Meaning |
| --- | --- |
| `-j`, `--jobs` | Filter by job IDs |
| `-s`, `--state` | Filter by state |
| `-S`, `--starttime` | Filter by start time |
| `-E`, `--endtime` | Filter by end time |
| `-u`, `--user` | Filter by user |
| `-p`, `--partition` | Filter by partition |
| `-o`, `--format` | Choose output fields |
| `-P`, `--parsable2` | Use `|`-delimited output |
| `-n`, `--noheader` | Omit the table header |

### `scancel`

| Option | Meaning |
| --- | --- |
| `--signal`, `-s` | Send a specific signal instead of cancelling normally |

### `scontrol update job`

| Key | Meaning |
| --- | --- |
| `JobName` / `Name` | Change the job name while `PENDING` |
| `Partition` | Change the partition while `PENDING` |
| `TimeLimit` / `Time` | Change the time limit before the job is terminal |
| `Priority` | Change local pending-job priority |

## Runtime Behavior

### Job states

Implemented states:

- `PENDING`
- `RUNNING`
- `COMPLETING`
- `COMPLETED`
- `FAILED`
- `CANCELLED`
- `TIMEOUT`
- `OUT_OF_MEMORY`

### Scheduling

- single-node only
- reservation-based CPU, memory, and GPU admission
- `ntasks` launches one local process per task rank for `sbatch`, foreground `srun`, and `salloc` commands
- pending jobs are ordered primarily by submission order
- explicit local `Priority` can override that order
- array tasks are interleaved by array group

### GPU detection

If `SLOTD_GPU_COUNT` is not set, `slotd` tries to detect GPUs automatically from `nvidia-smi`.

The current implementation checks common locations including:

- `nvidia-smi` from `PATH`
- `/usr/bin/nvidia-smi`
- `/usr/lib/wsl/lib/nvidia-smi`
- `/bin/nvidia-smi`

### Notifications

If `SLOTD_NOTIFY_CMD` is set, `slotd` runs it on terminal top-level job completion and exports:

- `SLOTD_JOB_ID`
- `SLOTD_JOB_NAME`
- `SLOTD_JOB_STATE`
- `SLOTD_JOB_PARTITION`
- `SLOTD_JOB_REASON`

Example:

```bash
./scripts/install.sh \
  --notify-cmd 'notify-send "slotd" "$SLOTD_JOB_ID $SLOTD_JOB_STATE"'
```

Expected result:

- when a top-level job reaches a terminal state, the configured notification command is executed

## Manual Daemon Startup

If you do not want to use `systemd --user`, run the daemon yourself:

```bash
cargo build --release
SLOTD_ROOT="$HOME/.local/share/slotd" ./target/release/slotd daemon
```

Then, in another shell:

```bash
SLOTD_ROOT="$HOME/.local/share/slotd" ./target/release/slotd sbatch --wrap 'echo hello'
```

## Testing

`slotd` is primarily covered by Rust integration tests under `tests/`.
Each test boots an isolated runtime under a temporary `SLOTD_ROOT`, starts its own daemon, and exercises the public Slurm-style commands without touching your normal local state.

Run the full suite:

```bash
cargo test
```

Run one integration test file while iterating on a feature:

```bash
cargo test --test scheduling
```

Run one named test case:

```bash
cargo test dependency_job_waits_for_prerequisite_before_running --test scheduling
```

Main areas covered by the current suite:

- command basics and CLI output such as `sbatch`, `srun`, `salloc`, `sinfo`, `squeue`, `sacct`, and `scontrol`
- scheduling behavior including dependencies, arrays, delayed start, resource flags, constraints, and requeue handling
- interactive and foreground execution paths such as `srun`, `--label`, `--unbuffered`, and allocation/step flows
- persistence and lifecycle behavior including cancellation, recovery, update processing, warning signals, and output file handling
- notification and accounting related behavior such as `SLOTD_NOTIFY_CMD` hooks and parsable query output

For a quick manual smoke test, run the daemon in one shell:

```bash
cargo run -- daemon
```

Then submit a simple job from another shell that uses the same `SLOTD_ROOT`:

```bash
cargo run -- sbatch --wrap 'echo hello'
```

## Current Boundaries

`slotd` intentionally does not try to be full Slurm.

Notable limits:

- no multi-node support
- no accounts, QoS, reservations, or fairshare
- no federation or cluster administration features
- `scontrol` is limited to job operations
- no full `sstat` or `sattach`
- only a subset of Slurm formatting tokens is implemented
