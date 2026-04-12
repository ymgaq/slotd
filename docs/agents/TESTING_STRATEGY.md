# slotd Testing Strategy

## Goal

`slotd` currently relies mostly on unit tests embedded in Rust source files.
That is useful for parser logic, formatting, and isolated state transitions, but
it does not sufficiently verify whether the scheduler works correctly as a user-
visible system.

This document defines the test strategy for moving from mostly unit-level checks
to a layered test suite that can validate real behavior locally and later in CI.

The target is to answer both of these questions:

- does each small piece of logic behave correctly
- does the whole feature work when exercised through the actual CLI and daemon

## Principles

- keep existing unit tests for fast feedback
- add integration tests that use the compiled binary as a black box
- test observable behavior instead of private implementation details
- isolate each test with its own runtime root
- make tests stable enough to run locally and in CI
- separate fast checks from slower end-to-end checks

## Test Layers

### 1. Unit Tests

Unit tests should remain the primary place for:

- pure parsing logic
- formatting logic
- state mapping helpers
- deterministic scheduling helpers
- edge cases that are easier to express without process setup

These tests should stay close to the code under test and remain fast.

Examples:

- `sbatch` directive parsing
- output column formatting
- resource normalization
- queue ordering helpers

### 2. Integration Tests

Integration tests should live under `tests/` and treat `slotd` as a real user
would: by invoking commands and observing outputs and persisted state.

These tests should verify feature behavior across module boundaries.

Examples:

- submit a job with `sbatch` and confirm it reaches `COMPLETED`
- submit a job with dependencies and verify it remains `PENDING` until cleared
- cancel a job with `scancel` and verify the final state is `CANCELLED`
- run `srun --immediate` and verify it fails when resources are unavailable
- verify array jobs expand and complete as expected
- verify `--requeue` only retries once

### 3. End-to-End Runtime Tests

Some behaviors should be tested with the daemon, SQLite state, socket
communication, and actual process execution all active.

These tests are effectively black-box system tests for a single-node runtime.

They are especially important for:

- daemon scheduling behavior
- process launch and adoption
- timeout enforcement
- signal handling
- cancellation of running jobs
- recovery after daemon restart

These tests may still live under `tests/`, but they should be written with clear
timeouts and isolated runtime roots so that they are safe for local execution
and CI.

## Repository Structure

The recommended layout is:

- `src/*.rs`
  Unit tests for pure logic and local invariants
- `tests/helpers/mod.rs`
  Shared integration-test helpers
- `tests/cli_basic.rs`
  Basic CLI workflows
- `tests/scheduling.rs`
  Dependency and `scontrol show job` behavior
- `tests/cancellation.rs`
  Pending and running cancellation and signal behavior
- `tests/requeue.rs`
  Requeue behavior
- `tests/recovery.rs`
  Daemon restart and adoption scenarios
- `tests/array.rs`
  Array expansion and concurrency-limit behavior
- `tests/srun.rs`
  `srun --immediate` resource checks
- `tests/srun_modes.rs`
  `srun --no-wait` and `salloc --immediate` behavior
- `tests/timeout.rs`
  Time-limit enforcement
- `tests/control.rs`
  `scontrol hold` / `release` behavior
- `tests/begin.rs`
  `--begin` scheduling behavior
- `tests/update.rs`
  `scontrol update` success and rejection cases
- `tests/constraint.rs`
  Constraint validation failures
- `tests/output_files.rs`
  Output and error file behavior
- `tests/query_output.rs`
  `squeue` and `sacct` filtering and formatting behavior
- `tests/sbatch_options.rs`
  `sbatch` export, open-mode, and chdir behavior
- `tests/dependency_variants.rs`
  Additional dependency modes and array-spec variants

This file structure can evolve, but the important point is to keep integration
tests organized by behavior, not by source file.

## Current Coverage

The current runtime-oriented test suite covers these user-visible workflows:

- basic `sbatch --wrap` submission through `COMPLETED`
- dependency-gated jobs staying `PENDING` and then completing
- `scontrol show job` core field rendering
- pending-job cancel
- running-job cancel and signal handling
- automatic requeue after failure
- `srun --immediate` failure when resources are unavailable
- `srun --no-wait` background submission
- `salloc --immediate` failure when resources are unavailable
- array task expansion and `%limit` concurrency control
- daemon restart, adoption, and minimal recovery behavior
- time-limit enforcement to `TIMEOUT`
- `scontrol hold` / `release`
- `--begin` scheduling delays
- `scontrol update` success cases while pending
- `scontrol update` rejection cases after completion
- constraint-validation failures
- default batch stdout files and foreground `srun` output/error redirection
- `squeue` filtering, sorting, long view, start view, historical view, and array view
- `sacct` filtering, parsable output, time bounds, and step/allocation record visibility
- `sbatch --export` and `--export-file`
- `sbatch --open-mode append|truncate`
- `sbatch --chdir`
- dependency modes `after`, `afterany`, `afternotok`, and `singleton`
- array stepped and mixed-segment specifications

## Shared Test Harness

All integration and end-to-end tests should use a common helper layer.

The helper module should provide:

- creation of an isolated temporary `SLOTD_ROOT`
- daemon startup for that isolated root
- daemon shutdown and cleanup
- helpers to invoke the compiled `slotd` binary
- helpers to poll `squeue`, `sacct`, or `scontrol` until a condition is met
- test timeouts with clear failure messages

Each test should be independent. No test should rely on state from any previous
test run.

## Runtime Isolation

Each integration test should create its own temporary runtime directory and set:

- `SLOTD_ROOT`

That isolates:

- the Unix socket
- the SQLite database
- job scripts
- logs and runtime files

This is mandatory for reliable local runs and future CI execution.

## What To Assert

Prefer assertions on observable behavior:

- exit status
- stdout and stderr
- queue state from `squeue`
- accounting state from `sacct`
- job details from `scontrol show job`
- files written under the isolated runtime root when they are part of behavior

Avoid assertions that depend on private function calls, helper names, or exact
internal control flow unless the test is intentionally a unit test.

## Polling and Timeouts

Scheduler behavior is asynchronous. Tests must not assume immediate state
transitions.

Use polling helpers instead of fixed sleeps whenever possible.

Recommended approach:

- issue a command
- poll for the expected state
- fail with a bounded timeout if the state never appears

Every integration test should have a clear upper timeout to avoid hanging
forever in local runs or CI.

## Recommended Crates

The following crates are appropriate for the integration test harness:

- `assert_cmd`
  for invoking the built binary
- `predicates`
  for stdout/stderr assertions
- `tempfile`
  for isolated runtime roots
- `serial_test`
  only if some runtime-sensitive tests cannot safely run in parallel

Add these as `dev-dependencies` when the harness is implemented.

## Initial Test Plan

The first milestone should cover one happy-path workflow and a few high-value
state transitions.

Recommended initial tests:

1. `sbatch` submits a trivial script and the job reaches `COMPLETED`
2. `sbatch --dependency` keeps the dependent job pending until the prerequisite finishes
3. `scancel` cancels a pending job
4. `sbatch --requeue` retries a failed job once and only once
5. `scontrol show job` returns the expected core fields

After that, add:

1. array job coverage
2. `srun --immediate` coverage
3. signal handling for running jobs
4. daemon recovery and adoption scenarios

Status:

- implemented:
  - trivial `sbatch -> COMPLETED`
  - dependency scheduling
  - additional dependency modes: `after`, `afterany`, `afternotok`, `singleton`
  - pending and running cancel flows
  - running-job signal flow
  - `scontrol show job`
  - `--requeue`
  - array coverage including `%limit`
  - stepped and mixed array specifications
  - `srun --immediate`
  - daemon recovery and adoption
  - timeout enforcement
  - `hold` / `release`
  - `--begin`
  - `scontrol update`
  - constraint rejection
  - output-file behavior
  - `sbatch` export, export-file, open-mode, and chdir behavior
- still useful future additions:
  - compound scenarios such as `array + dependency`
  - allocation-internal `srun` step behavior
  - warning-signal behavior from `--signal`
  - notification-hook behavior
  - OOM-specific behavior when a reliable local trigger exists

## Remaining Gaps

The current runtime suite covers the main scheduler paths, but several
feature-by-feature option surfaces are still thin.

Areas that still need stronger runtime coverage:

- `sbatch`
  - runtime validation of `-J`, `-p`, `-c`, `-n`, `--mem`, `-G`, and `--exclusive`
  - output-pattern expansion beyond the current default-path checks
- `srun`
  - `--pty`
  - `--label`
  - `--unbuffered`
  - `--cpu-bind none|cores|map_cpu:...`
  - allocation-internal step execution and step-record visibility
  - more runtime checks for `-D`, `-J`, `-p`, `-n`, `-G`, `--mem`, and `-t`
- `salloc`
  - allocation command behavior after grant
  - command omission / shell-launch behavior
  - more runtime checks for `-J`, `-p`, `-n`, `-G`, `--mem`, `-t`, `-D`, and `--constraint`
- `squeue`
  - `-u`
- `sacct`
  - richer field combinations beyond the current smoke coverage
- `scontrol`
  - successful reflection of `Priority` and `Partition` updates
  - update behavior combined with constraint validation
- `scancel`
  - step-target cancellation such as `<job_id>.<step_id>`
  - signal variants beyond the currently covered `TERM`
- `sinfo`
  - `-p`, `-N`, `-l`, `-o`, and `--noheader`

## Recommended Next Pass

The highest-value next additions are:

1. `srun` CPU binding and allocation-internal step behavior
2. runtime coverage for more `sbatch` resource flags such as `-J`, `-p`, `-c`, `-n`, `--mem`, `-G`, and `--exclusive`
3. compound scheduler cases such as `array + dependency`
4. deeper `scontrol` update coverage for `Priority` and `Partition`

## Local Workflow

The local development workflow should eventually distinguish between:

- fast checks
  - `cargo fmt --all -- --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - unit tests
- runtime integration checks
  - selected tests under `tests/`

Developers should be able to run a single integration test file while iterating,
without needing a full CI environment.

## Future CI Layout

This document does not require CI changes today, but the test suite should be
structured so it can later map cleanly onto separate CI jobs:

- lint and format
- unit tests
- integration tests
- heavier runtime or recovery tests

This separation keeps fast feedback fast while still allowing stronger behavior
validation before merge.

## Guidance For Writing New Tests

When adding a new test:

1. decide whether it is a unit, integration, or end-to-end runtime test
2. prefer the smallest layer that still validates the behavior meaningfully
3. isolate the runtime root
4. assert user-visible outcomes
5. add bounded polling and timeouts
6. keep scripts and commands minimal to reduce flakiness
7. update this document so the repository structure, implemented coverage, and
   remaining gaps stay accurate

Use simple commands such as:

- `true`
- `sleep 1`
- `exit 1`

Prefer deterministic commands over anything environment-dependent.

## Next Areas

The next high-value additions are:

1. compound scheduler scenarios such as `array + dependency`
2. allocation-backed `srun` step behavior after `salloc`
3. warning-signal behavior configured with `--signal`
4. notification-hook behavior
