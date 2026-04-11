# Runtime Model

## High-Level Model

`slotd` is a single-host scheduler. The runtime model is intentionally simple:

- one local daemon
- one local SQLite database
- one local execution host
- one local user workflow

There is no controller/worker split and no remote node launch protocol.

## Core Resources

`slotd` schedules three resource types:

- CPU
- memory
- GPU

Current behavior:

- CPU reservation is `ntasks * cpus-per-task`
- memory is stored in MB
- GPUs are integer slots
- admission is reservation-based, not usage-based

## Partitions

Configured by environment:

- `SLOTD_CPU_PARTITIONS`
- `SLOTD_GPU_PARTITIONS`

Rules:

- only configured partition names are accepted
- if there are no GPUs, no GPU partition is exposed
- if a GPU partition is selected and `--gpus` is omitted, the default GPU request is `1`
- otherwise the default GPU request is `0`

## GPU Detection

If `SLOTD_GPU_COUNT` is not set, `slotd` tries to detect GPUs from `nvidia-smi`.

The current implementation checks:

- `nvidia-smi`
- `/usr/bin/nvidia-smi`
- `/usr/lib/wsl/lib/nvidia-smi`
- `/bin/nvidia-smi`

## Job Types

Persisted records are one of:

- top-level batch jobs
- allocation-only jobs
- array tasks
- steps under allocations

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

Terminal states:

- `COMPLETED`
- `FAILED`
- `CANCELLED`
- `TIMEOUT`
- `OUT_OF_MEMORY`

## Scheduling Rules

The daemon loop runs every `300ms`.

Pending jobs are blocked by:

- dependencies
- array concurrency limits
- delayed start time
- exclusive host use
- insufficient reserved resources
- user hold state

Ordering:

- submission order is the base rule
- explicit job priority can override pure submission order
- array tasks are interleaved by array group

## Runtime Files

Within `SLOTD_ROOT`:

- `run/slotd.sock`: daemon socket
- `lib/state.db`: SQLite state
- `lib/jobs/<job_id>/script.sh`: batch script
- `lib/jobs/<job_id>/runner.sh`: daemon wrapper
- `lib/jobs/<job_id>/exit_status`: wrapper exit status

## Notifications

If `SLOTD_NOTIFY_CMD` is set, `slotd` runs it for terminal top-level jobs.

Exported variables:

- `SLOTD_JOB_ID`
- `SLOTD_JOB_NAME`
- `SLOTD_JOB_STATE`
- `SLOTD_JOB_PARTITION`
- `SLOTD_JOB_REASON`
