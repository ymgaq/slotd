# slotd Design Overview

## Goal

`slotd` is a single-node job scheduler written in Rust for personal or small-scale use.
It should feel familiar to Slurm users at the CLI level, while staying much simpler
internally and easier to operate on one machine.

The target is not Slurm compatibility. The target is a Slurm-like UX with a design
that is optimized for:

- single node
- single user or tightly scoped local use
- simple deployment
- reliable restart and recovery
- readable CLI output

## Core Design Principles

### Slurm-like UX, not Slurm compatibility

The CLI should use familiar commands such as:

- `sbatch`
- `srun`
- `squeue`
- `sinfo`
- `scancel`

The command names, major options, and mental model should feel close to Slurm.
However, internal behavior can be simplified whenever that improves clarity,
reliability, or implementation speed.

### Clean responsibility split

`systemd` should manage only the `slotd` daemon lifecycle.
The daemon should own scheduling and job execution behavior.

- `systemd`
  - start the daemon at boot
  - restart the daemon on failure
  - collect daemon logs
- `slotd` daemon
  - accept job submissions
  - manage queue and state transitions
  - persist state
  - schedule runnable jobs
  - launch and monitor processes
  - handle cancellation
- CLI tools
  - submit, inspect, and control jobs through IPC

### One binary, multiple command names

Use a single executable and dispatch behavior by argv[0] or subcommand name.
Install symlinks such as:

- `sbatch -> slotd`
- `srun -> slotd`
- `squeue -> slotd`
- `sinfo -> slotd`
- `scancel -> slotd`

This keeps distribution and upgrades simple while preserving familiar command names.

## High-Level Architecture

### Main components

- daemon
  - Unix domain socket server for CLI requests
  - owns in-memory scheduler state
  - persists durable state to SQLite
- scheduler loop
  - scans pending jobs
  - checks available reserved resources
  - starts jobs when resources are available
- runner
  - launches each job
  - separates process groups
  - redirects stdout and stderr to files
- reaper
  - detects process exit
  - records exit status
  - updates job state
- recovery manager
  - restores state after daemon restart
  - reconnects to still-running jobs where possible

### Suggested Rust workspace layout

```text
slotd/
├── Cargo.toml
├── crates/
│   ├── slotd-core/
│   ├── slotd-proto/
│   ├── slotd-daemon/
│   └── slotd-cli/
└── packaging/
    └── systemd/
```

Recommended crate responsibilities:

- `slotd-core`
  - domain models
  - scheduler policy interface
  - state transition logic
  - resource accounting
- `slotd-proto`
  - request and response types for CLI-daemon IPC
- `slotd-daemon`
  - socket server
  - scheduler loop
  - job launcher and reaper
  - persistence and recovery
- `slotd-cli`
  - command parsing
  - output formatting
  - symlink-aware command dispatch

## Runtime Model

### IPC

Use a Unix domain socket between CLI and daemon.

Why:

- local-only by default
- simple permissions model
- no need for a network service
- easy fit for single-node operation

Suggested socket path:

- `/run/slotd/slotd.sock`

### State storage

Use SQLite from the beginning.

Why:

- stable `squeue` behavior
- easy restart recovery
- no need to invent a file format
- enough durability for a single-node scheduler

Suggested state path:

- `/var/lib/slotd/state.db`

### Job artifacts

Store per-job files in dedicated directories.

Suggested layout:

- `/var/lib/slotd/jobs/<job_id>/script.sh`
- `/var/lib/slotd/jobs/<job_id>/stdout.log`
- `/var/lib/slotd/jobs/<job_id>/stderr.log`

## Job Model

Each job should carry enough metadata for scheduling, recovery, and CLI display.

Suggested fields:

- `job_id`
- `name`
- `state`
- `submit_time`
- `start_time`
- `end_time`
- `command`
- `script_path`
- `cwd`
- `env`
- `requested_cpus`
- `requested_memory_mb`
- `priority`
- `pid`
- `pgid`
- `exit_code`
- `stdout_path`
- `stderr_path`
- `cancel_requested_at`

Not every field must be user-visible in early versions, but the model should be
designed so that CLI output and recovery logic can rely on it.

## Job States

Start with a minimal state machine:

- `PENDING`
- `RUNNING`
- `COMPLETED`
- `FAILED`
- `CANCELLED`

Possible later additions:

- `CANCELING`
- `TIMEOUT`
- `NODE_FAIL`

Internally, it may still be useful to track a short-lived cancellation-requested
phase, but the initial external state model should stay simple.

## Scheduling and Resource Management

### Initial scheduling policy

Start with FIFO scheduling.

Important design point:

- implement FIFO first
- define the scheduler behind an interface so policy can change later

For example, policy selection can later be extended to support:

- priority-based scheduling
- fair-share-like heuristics
- age-based queue improvements

### Initial resource model

Treat CPU and memory as reservation values for scheduling decisions.

- CPU
  - track logical slots
- memory
  - track requested megabytes as reserved capacity

This first version should decide whether a job is allowed to start, not enforce
strict runtime isolation.

### Future resource enforcement

Add `cgroup v2` later when stronger isolation becomes necessary.

Good second-stage targets:

- `memory.max`
- `cpu.max`

This should be a later phase, not part of the MVP.

## Process Execution Model

The daemon should launch jobs directly.

Execution behavior:

- spawn the job as its own process group
- redirect stdout and stderr to per-job files
- record `pid` and `pgid`
- monitor exit and update persistent state

Cancellation behavior:

- `scancel` sends `SIGTERM` to the process group
- wait for a short grace period
- if still alive, send `SIGKILL`

Tracking process groups is important so cancellation applies to the whole job tree,
not only the immediate child process.

## Recovery Strategy

Daemon restart recovery should be part of the initial design.

On startup:

1. Load jobs in `RUNNING` state from SQLite.
2. Check whether their `pid` or `pgid` is still alive.
3. If the process still exists, restore monitoring state.
4. If it no longer exists, finalize the job as `FAILED` or `COMPLETED` based on the
   best available information.

This keeps `squeue` stable after daemon restart and avoids confusing orphaned state.

## CLI Design

### General goals

The CLI should be readable by default, not just compatible.

Recommended behavior:

- human-friendly fixed-width tables by default
- `--json` for machine-readable output
- colorized states where appropriate
- short default columns with a `--wide` or `--long` mode
- optional watch mode for queue inspection

### Suggested command scope

- `sbatch`
  - submit a script for asynchronous execution
- `srun`
  - submit a command for immediate execution
  - optionally wait for resources or fail immediately
- `squeue`
  - inspect job state
- `sinfo`
  - show single-node resource view
- `scancel`
  - request job cancellation

### Output philosophy

Prefer output that feels familiar to Slurm users but is easier to scan.
For example, `squeue` can keep recognizable columns while improving defaults:

- `JOBID`
- `NAME`
- `ST`
- `CPUS`
- `MEM`
- `ELAPSED`
- `COMMAND`

## `sbatch` and `srun` Scope

These commands should be intentionally limited in the MVP.

### `sbatch`

- accepts a shell script
- stores a copy in the job directory
- parses a minimal subset of `#SBATCH` directives
- submits the job asynchronously

Initial `#SBATCH` support:

- `--job-name`
- `--cpus-per-task`
- `--mem`
- `--output`
- `--error`

### `srun`

- accepts a direct command invocation
- submits it to the scheduler immediately
- either waits for resources or fails fast with an option such as `--immediate`

## systemd Integration

Use `systemd` to supervise only the daemon, not every individual job.

Suggested service shape:

```ini
[Unit]
Description=slotd job scheduler daemon
After=network.target

[Service]
ExecStart=/usr/local/bin/slotd daemon
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

This keeps the architecture simple. If tighter integration is needed later, jobs can
eventually be mapped to systemd scopes or units, but that should not be required for
the first usable release.

## Recommended Rust Stack

Rust is the primary recommendation for this project.

Why Rust fits well:

- safe long-running daemon implementation
- strong modeling of state transitions with enums
- easy single-binary distribution
- no external runtime requirement
- good fit for CLI plus daemon plus process-control code in one codebase

Suggested libraries:

- `clap`
- `tokio`
- `serde`
- `rusqlite` or `sqlx` with SQLite
- `nix`

## MVP Roadmap

Recommended implementation order:

1. build `slotd daemon` with SQLite initialization
2. add `sbatch`
3. add `squeue`
4. implement FIFO scheduling and job launch
5. add `scancel`
6. add daemon restart recovery
7. add `srun`
8. add `sinfo`
9. add `cgroup v2` support

This sequence reaches a useful scheduler quickly while leaving room for stronger
resource control and richer compatibility later.

## Final Recommendation

The most practical initial design for `slotd` is:

- Rust implementation
- one executable with symlinked Slurm-like command names
- Unix domain socket IPC
- SQLite-backed durable state
- FIFO scheduling
- reservation-based CPU and memory accounting
- process-group-based job control
- `systemd` supervising only the daemon

This keeps the product small, robust, and understandable while still delivering the
Slurm-like workflow that users expect.
