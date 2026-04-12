# Testing

`slotd` is mainly verified with Rust integration tests in `tests/`.
Each test creates an isolated temporary runtime, launches its own daemon, and drives the public Slurm-style commands through the compiled `slotd` binary.
That keeps test state separate from your normal local `SLOTD_ROOT`.

## How to Run the Tests

Run the full suite:

```bash
cargo test
```

Run one integration test file:

```bash
cargo test --test scheduling
```

Run one named test:

```bash
cargo test dependency_job_waits_for_prerequisite_before_running --test scheduling
```

## What the Tests Cover

The current suite focuses on behavior that matters to end users:

- core command flows for `sbatch`, `srun`, `salloc`, `sinfo`, `squeue`, `sacct`, and `scontrol`
- scheduling rules such as dependencies, job arrays, delayed start, constraints, resource flags, and requeue behavior
- interactive and foreground execution paths including `srun --pty`, `--label`, `--unbuffered`, and allocation or step handling
- output, recovery, and lifecycle behavior including cancellation, warning signals, update processing, and output file placement
- notification and reporting paths such as `SLOTD_NOTIFY_CMD` and parsable query output

Representative files in `tests/` include:

- `cli_basic.rs`, `sbatch_options.rs`, `srun.rs`, `srun_options.rs`, `srun_modes.rs`, `salloc.rs`, `sinfo.rs`, `control.rs`, `query_output.rs`
- `scheduling.rs`, `compound_scheduling.rs`, `dependency_variants.rs`, `array.rs`, `begin.rs`, `constraint.rs`, `resource_flags.rs`, `requeue.rs`, `timeout.rs`
- `srun_interactive.rs`, `srun_allocation.rs`, `cpu_bind.rs`, `output_files.rs`, `cancellation.rs`, `recovery.rs`, `update.rs`, `warning_signal.rs`, `notify.rs`

## Manual Smoke Testing

For a quick manual check, start the daemon:

```bash
cargo run -- daemon
```

Then submit a small job from another shell using the same `SLOTD_ROOT`:

```bash
cargo run -- sbatch --wrap 'echo hello'
```
