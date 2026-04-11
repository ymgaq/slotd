# slotd Design Overview

## Goal

`slotd` is a single-node scheduler that should preserve as much of the Slurm user
experience as is practical on one machine.

The target is no longer merely "Slurm-like". The target is:

- familiar Slurm command names
- familiar Slurm option names
- familiar Slurm default behaviors where feasible
- a simpler internal implementation than Slurm
- constraints that are explicit when single-node operation makes full compatibility impossible

In short, `slotd` should be understandable as a local single-node Slurm subset,
not as an unrelated scheduler that happens to reuse some command names.

## Current Implementation Snapshot

The repository already implements:

- one Rust binary with argv[0]-based command dispatch
- `sbatch`
- `srun`
- `squeue`
- `sacct`
- `scancel`
- `sinfo`
- a local daemon using a Unix domain socket
- SQLite-backed job persistence
- FIFO scheduling
- reservation-based CPU, memory, and GPU admission
- process-group-based cancellation
- minimal restart recovery for running jobs

The implemented runtime state machine is currently:

- `PENDING`
- `RUNNING`
- `COMPLETED`
- `FAILED`
- `CANCELLED`

The implemented system is usable, but it is still an MVP and diverges from Slurm
in several important ways, especially around `srun`, partition modeling, default
output behavior, and option coverage.

## Design Direction

### Compatibility First at the CLI Boundary

The highest priority is to align command-line interface and observable behavior
with Slurm where that does not fundamentally conflict with the single-node model.

This means:

- prefer Slurm command names over custom names
- prefer Slurm option names and short flags over custom options
- prefer Slurm defaults over project-specific defaults
- prefer Slurm output columns and state labels over custom table design
- preserve Slurm mental models even if implementation is simplified internally

This does not mean reproducing all of Slurm internals.

It does mean that when a user already knows Slurm, the obvious command should
usually work the obvious way.

### Explicit Single-Node Scope

`slotd` is still intentionally single-node.

That implies:

- no controller / compute-node split
- no distributed launch protocol
- no real multi-node allocations
- one daemon owns scheduling and execution on one host
- one local database is the source of truth

When Slurm semantics cannot be reproduced exactly because of this scope, the
difference should be documented and the user-visible interface should still stay
as close as possible.

## Core Principles

### One Binary, Slurm-Named Entrypoints

Use one executable and dispatch by argv[0] or subcommand name.

Expose the binary through symlinks such as:

- `sbatch -> slotd`
- `srun -> slotd`
- `squeue -> slotd`
- `sacct -> slotd`
- `scancel -> slotd`
- `sinfo -> slotd`

This keeps packaging simple while matching how users expect these commands to
exist on a system.

### Daemon Owns Scheduling and Execution

`systemd` should supervise the daemon, not each job.

Responsibility split:

- `systemd`
  - start the daemon
  - restart the daemon on failure
  - collect daemon logs
- `slotd` daemon
  - own queue state
  - schedule jobs
  - launch and monitor processes
  - persist state transitions
  - handle cancellation and recovery
- CLI tools
  - parse Slurm-like options
  - send requests to the daemon
  - render Slurm-like output

### Internal Simplicity, External Familiarity

The implementation can stay small if internal data structures are designed
around local execution, but the CLI should not drift away from Slurm unless
there is a concrete reason.

Examples:

- SQLite is acceptable even though Slurm uses different internals
- a simple FIFO scheduler is acceptable initially
- process groups are an acceptable local substitute for a more complex launcher
- custom command names or custom flag names are not desirable

## Main Components

- daemon
  - Unix domain socket server
  - scheduler loop
  - persistent state owner
- store
  - SQLite-backed job metadata
  - resource accounting
  - query layer for queue and accounting views
- runner
  - process launch
  - stdout and stderr setup
  - process-group-based cancellation
  - exit detection
- recovery
  - reattach best-effort running job state after daemon restart
- CLI
  - Slurm-style argument parsing
  - argv[0] dispatch
  - Slurm-style output rendering

## Runtime Model

### IPC

Use a local Unix domain socket between CLI and daemon.

Why:

- local-only by default
- simple security model
- easy to package and supervise
- aligns with single-node scope

Default path target:

- `/run/slotd/slotd.sock`

The current repository uses `var/run/slotd.sock` for local development. Packaging
should move toward `/run/slotd/slotd.sock`.

### Durable State

SQLite is the source of truth for job metadata.

Why:

- restart recovery needs durable job state
- `squeue` and `sacct` need stable reads
- single-node usage does not justify a more complex store

Default path target:

- `/var/lib/slotd/state.db`

The current repository uses `var/lib/state.db` for local development.

### Job Artifacts

Each job should have a dedicated directory.

Target layout:

- `/var/lib/slotd/jobs/<job_id>/script.sh`
- `/var/lib/slotd/jobs/<job_id>/stdout.log`
- `/var/lib/slotd/jobs/<job_id>/stderr.log`

This remains useful even if default Slurm-style output naming is added, because
the job directory is still the safest place for internal artifacts.

## Job Model

Each job record should carry enough metadata to support:

- scheduling
- user-visible queue output
- accounting output
- cancellation
- restart recovery
- future compatibility work

Minimum job fields:

- `job_id`
- `name`
- `user_name`
- `state`
- `partition`
- `submit_time`
- `start_time`
- `end_time`
- `command`
- `script_path`
- `cwd`
- `requested_cpus`
- `requested_memory_mb`
- `requested_gpus`
- `stdout_path`
- `stderr_path`
- `pid`
- `pgid`
- `exit_code`
- `assigned_gpu_ids`

Planned future fields:

- `state_reason`
- `signal`
- `time_limit`
- `requested_nodes`
- `requested_tasks`
- `dependency`
- `array_job_id`
- `array_task_id`
- `submit_host`
- `open_mode`
- `priority`

The future fields matter because several Slurm-visible behaviors depend on them,
even if the initial scheduler logic does not.

## Job States

### Current States

- `PENDING`
- `RUNNING`
- `COMPLETED`
- `FAILED`
- `CANCELLED`

### Target States

To align better with Slurm output and accounting, the job model should expand to
support at least:

- `PENDING`
- `RUNNING`
- `COMPLETING`
- `COMPLETED`
- `FAILED`
- `CANCELLED`
- `TIMEOUT`
- `OUT_OF_MEMORY`

Potential later additions:

- `NODE_FAIL`
- `PREEMPTED`
- `SUSPENDED`

The point is not to implement every Slurm state immediately. The point is to stop
encoding all non-successful terminal outcomes as generic `FAILED`.

## Scheduling and Resource Management

### Initial Scheduler Policy

Continue to use FIFO as the first scheduling policy.

This is acceptable because:

- it is simple
- it is predictable
- it matches current implementation
- it does not block CLI compatibility work

The scheduler should still be written so that alternative policies can be added
later without rewriting persistence or command handling.

### Resource Accounting

CPU, memory, and GPU requests are currently reservation values for admission.

This remains the right initial model, but it should evolve to match Slurm terms
more closely:

- `--cpus-per-task`
- `--ntasks`
- `--nodes`
- `--mem`
- `--gpus`
- `--gres`

Even if some of these collapse internally into a simpler single-node admission
calculation, the CLI should accept and store them in Slurm terms.

### Resource Enforcement

Runtime enforcement is a later phase.

Recommended later targets:

- `cgroup v2` memory control
- CPU quota or affinity control
- job-specific process accounting

Compatibility work should come before strict enforcement because the current gap
is larger at the interface layer than at the isolation layer.

## Partition Model

The current implementation effectively exposes only one active partition at a
time and validates only the default partition.

That is too far from Slurm semantics.

The target partition model should be:

- allow multiple configured partitions to exist simultaneously
- allow a CPU-oriented partition and a GPU-oriented partition to coexist
- preserve one-machine backing resources underneath
- validate partition names directly, not only against a single default
- expose partition state through `sinfo`

This does not require multiple nodes. It only requires a more faithful mapping
between user requests and scheduler-visible partition metadata.

## Process Execution Model

Jobs should continue to run as locally spawned processes.

Required behavior:

- each job gets its own process group
- cancellation targets the whole process group
- stdout and stderr are redirected or streamed according to command semantics
- exit status is recorded in durable state

### `sbatch`

`sbatch` should remain an asynchronous submission command.

Target behavior:

- accept `script [args...]`
- accept `--wrap`
- parse a useful subset of `#SBATCH`
- respect Slurm-like precedence
  - CLI options
  - `#SBATCH`
  - environment
  - built-in defaults
- print Slurm-compatible submission output

### `srun`

`srun` is the largest current compatibility gap.

The current implementation treats `srun` as asynchronous command submission.
That is not close enough to actual Slurm behavior.

Target behavior:

- default to foreground execution semantics
- wait until resources are available unless configured otherwise
- return the command exit code
- support `--immediate` failure when resources are not available
- later support `--pty` and direct stdio streaming

Internally, `srun` may still create a scheduler-managed job record, but to the
user it should feel like Slurm `srun`, not like a second spelling of `sbatch`.

## Recovery Strategy

Restart recovery remains a core requirement.

On daemon startup:

1. load jobs in active states from SQLite
2. inspect whether their process or process group still exists
3. reattach monitoring state when possible
4. finalize jobs that are no longer alive using the best available evidence

Current recovery is best-effort only. In particular, adopted jobs that finish
after daemon restart cannot always recover an exact terminal state.

The design goal is:

- keep queue state coherent after restart
- avoid orphaned `RUNNING` jobs
- improve terminal-state accuracy over time

## CLI Design

### General Rule

Default output should be as recognizable to Slurm users as possible.

Avoid inventing new table layouts unless there is a strong reason.

### `sbatch`

High-priority compatibility targets:

- `-J`, `--job-name`
- `-p`, `--partition`
- `-c`, `--cpus-per-task`
- `-G`, `--gpus`
- `-o`, `--output`
- `-e`, `--error`
- `-D`, `--chdir`
- `-W`, `--wait`
- `--wrap`
- `--parsable`

### `srun`

High-priority compatibility targets:

- `-J`, `--job-name`
- `-p`, `--partition`
- `-c`, `--cpus-per-task`
- `-G`, `--gpus`
- `-o`, `--output`
- `-e`, `--error`
- `-D`, `--chdir`
- `--immediate`
- `--pty`
- `-n`, `--ntasks`

### `squeue`

High-priority compatibility targets:

- `-u`, `--user`
- `-j`, `--jobs`
- `-p`, `--partition`
- `-t`, `--states`
- `-o`, `--format`
- `-h`, `--noheader`
- `-S`, `--sort`
- `-l`, `--long`

### `sacct`

High-priority compatibility targets:

- `-j`, `--jobs`
- `-s`, `--state`
- `-S`, `--starttime`
- `-E`, `--endtime`
- `-o`, `--format`
- `-n`, `--noheader`
- `-p`, `--parsable2`

### `scancel`

High-priority compatibility targets:

- cancel by job ID
- `--signal`
- `-u`, `--user`
- `-p`, `--partition`
- `-t`, `--state`
- `-n`, `--name`

### `sinfo`

High-priority compatibility targets:

- `-p`, `--partition`
- `-N`, `--Node`
- `-l`, `--long`
- `-o`, `--format`
- `-h`, `--noheader`

## Output Design

### Prefer Slurm Defaults

User-facing defaults should converge toward Slurm, including:

- `sbatch` submission message format
- `sbatch --parsable` output
- `squeue` default columns
- `sacct` default fields
- `sinfo` default columns
- state abbreviations such as `PD`, `R`, `CG`, `CD`, `CA`, `F`, `TO`, `OOM`

### Default Log Paths

The current implementation writes default logs into per-job directories.

For better compatibility, the target behavior should move toward Slurm defaults:

- default stdout path like `slurm-%j.out`
- stderr behavior that matches Slurm semantics more closely
- support for `%j`, `%x`, and other common output path substitutions

Internal job directories should remain available for implementation needs, but
user-visible default file naming should prioritize compatibility.

## `#SBATCH` Parsing Rules

The `#SBATCH` parser should align with Slurm behavior more closely.

Required rules:

- only lines beginning with `#SBATCH` are directives
- only the initial comment block is considered
- once the parser reaches the first non-comment, non-blank executable line,
  later `#SBATCH` lines are ignored
- CLI options override `#SBATCH`
- environment can later override defaults where appropriate

The parser should also grow support for common short forms and common value
styles rather than only a few long options.

## Non-Goals

The following are not current goals:

- full multi-node Slurm behavior
- full Slurm RPC protocol compatibility
- reproducing every Slurm option in early phases
- reproducing cluster administration features
- reproducing Slurm controller and slurmd split

The design goal is practical user-level command compatibility for a local
single-node scheduler.

## Supported Subset Strategy

`slotd` is not trying to become a full Slurm reimplementation.

The intended product is:

- a Slurm-shaped scheduler for one personal machine
- one local daemon
- one local user
- one local execution host
- a deliberately limited subset of Slurm commands and options

This has a few design consequences.

First, compatibility should be strongest where individual users actually spend
their time:

- batch submission
- interactive execution
- allocation-based execution
- queue inspection
- accounting inspection
- cancellation
- per-job detail inspection
- local resource inspection

Second, features whose value depends primarily on multi-user or multi-node
operation should not be implemented just because they exist in upstream Slurm.

Third, the supported surface area should be explicit and stable enough that a
user can tell which Slurm habits transfer directly and which do not.

### Supported Commands

The intended supported command set is:

- `sbatch`
- `srun`
- `salloc`
- `squeue`
- `sacct`
- `scancel`
- `scontrol`
- `sinfo`

Within `scontrol`, the supported subset is intentionally narrow:

- `show job`
- `hold job`
- `release job`
- `update job`

### Supported Core Options

The common resource and execution model should center on:

- `-J`, `--job-name`
- `-p`, `--partition`
- `-c`, `--cpus-per-task`
- `-n`, `--ntasks`
- `--mem`
- `-t`, `--time`
- `-G`, `--gpus`
- `-D`, `--chdir`

The command-specific high-value options are:

- `sbatch`
  - `--wrap`
  - `-o`, `--output`
  - `-e`, `--error`
  - `-d`, `--dependency`
  - `-a`, `--array`
  - `--parsable`
  - `-W`, `--wait`
- `srun`
  - `-o`, `--output`
  - `-e`, `--error`
  - `--immediate`
  - `--pty`
- `salloc`
  - `--immediate`
- `squeue`
  - `--all`
  - `-j`, `--jobs`
  - `-u`, `--user`
  - `-p`, `--partition`
  - `-t`, `--states`
  - `-o`, `--format`
  - `-S`, `--sort`
  - `-l`, `--long`
  - `--noheader`
- `sacct`
  - `-j`, `--jobs`
  - `-s`, `--state`
  - `-S`, `--starttime`
  - `-E`, `--endtime`
  - `-u`, `--user`
  - `-p`, `--partition`
  - `-o`, `--format`
  - `-P`, `--parsable2`
  - `-n`, `--noheader`
- `scancel`
  - cancel by job ID
  - `--signal`
- `sinfo`
  - `-p`, `--partition`
  - `-N`, `--Node`
  - `-l`, `--long`
  - `-o`, `--format`
  - `--noheader`

### Supported Near-Term Extensions

The next useful additions within this subset are:

- `sbatch --export`
- `sbatch --export-file`
- `sbatch --open-mode`
- `sbatch --signal`
- `sbatch --begin`
- `sbatch --exclusive`
- `srun --cpu-bind`
- `srun --label`
- `srun --unbuffered`
- `squeue --start`
- shared `--constraint` support across `sbatch`, `srun`, and `salloc`

These remain compatible with the single-user, single-host model and directly
improve local experimentation, reproducibility, and observability.

## Features We Should Not Add

The following features should be treated as explicitly out of scope unless the
product definition changes.

### Multi-User And Cluster Administration Features

- `sacctmgr`
- `sreport`
- `sshare`
- `sprio`
- account hierarchy management
- fairshare configuration
- QoS policy management
- reservation administration

These are mainly valuable in shared clusters. They add substantial conceptual
surface area without improving the personal-machine workflow.

### Multi-Node And Federation Features

- true multi-node allocations
- `slurmctld` / `slurmd` role separation
- federation
- `--clusters`
- remote launch protocols
- topology-aware network scheduling
- cross-node task placement semantics

These would fundamentally change the architecture and should not be approached
incrementally under the current product scope.

### Low-Value Slurm Surface For This Product

- `sbcast`
- `strigger`
- `sdiag`
- `sh5util`
- `sview`
- `scrun`
- full `sattach`
- full `scrontab`

Some of these may be useful in full Slurm deployments, but they are not core to
local batch, interactive, and inspection workflows on one machine.

### Options We Should Not Implement

The following option families should remain unsupported unless there is a clear
single-node use case that justifies them:

- `--account`
- `--qos`
- `--reservation`
- `--licenses`
- `--wckey`
- `--mcs-label`
- `--clusters`
- `--nodes`
- `--nodelist`
- `--exclude`
- `--ntasks-per-node`
- `--ntasks-per-socket`
- `--ntasks-per-core`
- `--gpus-per-node`
- `--gpus-per-socket`
- `--gpus-per-task`
- `--gres`
- `--tres-*`
- burst buffer options
- power management options
- OCI / container integration options
- federation / sibling options

The rule is simple: if an option is mainly meaningful because Slurm is managing
many nodes, many users, or many administrative domains, it should not be in the
target subset.

## Roadmap

The roadmap should follow the chosen single-user subset, not upstream Slurm's
full command surface.

### Phase 0: Freeze The Product Boundary

1. document the supported command set
2. document the supported option families
3. document explicit non-goals and excluded Slurm features
4. define one shared resource model across `sbatch`, `srun`, and `salloc`
5. stop adding ad hoc options outside that model

Completion criteria:

- the project has a stable definition of its intended Slurm subset
- future compatibility work can be judged against an explicit scope boundary

### Phase 1: Stabilize The Existing Core Commands

1. make `sbatch`, `srun`, and `salloc` interpret common resource flags consistently
2. keep `srun` foreground and exit-code-preserving by default
3. keep `salloc` allocation-first and shell-friendly
4. stabilize `squeue`, `sacct`, `scontrol show job`, and `sinfo` default output
5. ensure `#SBATCH` parsing and precedence rules are documented and predictable

Completion criteria:

- the existing command set is coherent enough for day-to-day local use
- users do not have to remember command-specific quirks for the same resource flags

### Phase 1.5: Reconcile Existing Behavior With The Intended Subset

This phase exists to correct already-implemented behavior that is either too far
from Slurm semantics or unnecessarily broad for the personal-machine subset.

1. fix `sbatch` precedence so it matches the intended order
   - CLI options
   - environment
   - `#SBATCH`
   - built-in defaults
2. narrow `srun` behavior so it feels like one command with predictable foreground semantics rather than multiple hidden execution modes
3. add `scancel --signal` and treat it as part of the core subset rather than an optional enhancement
4. decide whether `scontrol hold/release/update` remains part of the supported subset or is reduced to a smaller stable surface
5. decide whether explicit job priority remains part of the product
6. keep array jobs and step tracking only to the extent they improve local usability, not to chase full Slurm internal parity
7. remove or simplify behaviors whose primary purpose is shared-cluster scheduling rather than single-user local execution

Design guidance for this phase:

- fix incompatible behavior before adding more compatible-looking surface area
- prefer deleting or narrowing weakly justified behavior over preserving it for its own sake
- if a feature is kept, it should either improve local usability directly or strengthen the supported Slurm subset
- if a feature is kept under the same name as Slurm, its behavior should not be surprisingly different

Completion criteria:

- the most user-visible semantic mismatches in existing commands are corrected
- features that do not justify their maintenance cost in the single-user model are either removed, narrowed, or explicitly demoted
- the project no longer carries ambiguous features that are neither clearly supported nor clearly out of scope

### Phase 2: Add Reproducibility And Job-Control Essentials

1. add `scancel --signal`
2. add `sbatch --export`
3. add `sbatch --export-file`
4. add `sbatch --open-mode=append|truncate`
5. add `sbatch --signal`
6. add `squeue --start`

Completion criteria:

- users can control shutdown behavior cleanly
- users can make job environments more reproducible
- users can reason about waiting jobs more easily

### Phase 3: Add High-Value Single-Node Placement Controls

1. add shared `--constraint` support to `sbatch`, `srun`, and `salloc`
2. add `srun --cpu-bind`
3. improve `sinfo -l`
4. stabilize `scontrol update job` for `JobName`, `Partition`, `TimeLimit`, and `Priority`
5. keep these controls faithful to the single-host model rather than emulating multi-node behavior

Completion criteria:

- users can target local hardware characteristics intentionally
- CPU-heavy local runs can be made more stable and predictable
- post-submission adjustments remain limited and understandable

### Phase 4: Add Convenience Features That Help Daily Workflows

1. add `sbatch --begin`
2. add `sbatch --exclusive`
3. add `srun --label`
4. add `srun --unbuffered`
5. improve array display and accounting polish where it helps local workflows

Completion criteria:

- common personal-machine workflows become smoother without widening scope
- convenience features do not require a redesign of the scheduler model

### Phase 5: Add Optional Extensions Only If Real Demand Appears

1. consider `--requeue`
2. consider lightweight notification support
3. consider a minimal `sstat`-like running-statistics view
4. consider a narrow `sattach`-like capability only if needed for local debugging
5. reject additions that mostly serve shared-cluster administration

Completion criteria:

- every feature in this phase has a demonstrated local-user need
- the product remains a focused personal-machine scheduler rather than drifting toward full Slurm

## Next Implementation Order

The recommended order for future work is:

1. Phase 0
2. Phase 1
3. Phase 1.5
4. Phase 2
5. Phase 3
6. Phase 4
7. Phase 5

This order is intentional.

The supported subset should be frozen first so that implementation work does not
expand the surface area accidentally. After that, the existing commands should
be made coherent before adding new options. Existing incompatible or weakly
justified behavior should be reconciled before expanding the surface further.
Reproducibility, shutdown control, and local observability come before
convenience features.

## Final Recommendation

The most practical design for `slotd` is:

- one executable with Slurm-named symlink entrypoints
- one local daemon supervised by `systemd`
- Unix domain socket IPC
- SQLite as durable state
- FIFO scheduling initially
- single-node resource accounting
- process-group-based execution and cancellation
- strong emphasis on Slurm-compatible command interface and observable behavior

The main correction from earlier design direction is this:

`slotd` should not optimize for "custom but pleasant" CLI behavior first.
It should optimize for "predictably Slurm-like" behavior first, then simplify the
internals behind that interface where single-node operation allows it.
