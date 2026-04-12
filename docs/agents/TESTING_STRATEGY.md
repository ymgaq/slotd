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

- `src/app/`, `src/model/`, `src/proto/`, `src/util/`
  Shared application, domain, protocol, and utility modules with unit tests
- `src/runtime/`
  Daemon, launch, recovery, and process-execution modules with unit tests where practical
- `src/command/`, `src/format/`, `src/submit/`, `src/store/`
  CLI, output formatting, submission parsing, and persistence modules with unit tests
- `tests/helpers/mod.rs`
  Shared integration-test helpers
- `tests/cli_basic.rs`
  Basic CLI workflows
- `tests/scheduling.rs`
  Dependency and `scontrol show job` behavior
- `tests/cancellation.rs`
  Pending and running cancellation, signal behavior, and step-target cancellation
- `tests/warning_signal.rs`
  Warning-signal delivery before time limits
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
- `tests/notify.rs`
  Notification-hook behavior
- `tests/output_files.rs`
  Output and error file behavior
- `tests/sinfo.rs`
  `sinfo` filtering and rendering behavior
- `tests/query_squeue.rs`
  `squeue` filtering, sorting, and historical/array views
- `tests/query_sacct.rs`
  `sacct` filtering, parsable output, and richer field combinations
- `tests/sbatch_options.rs`
  `sbatch` export, open-mode, and chdir behavior
- `tests/dependency_variants.rs`
  Additional dependency modes and array-spec variants
- `tests/srun_allocation.rs`
  Allocation-internal `srun` step behavior
- `tests/srun_interactive.rs`
  Interactive `srun` foreground I/O behavior
- `tests/cpu_bind.rs`
  `srun --cpu-bind` behavior
- `tests/salloc.rs`
  `salloc` shell launch and resource-flag behavior
- `tests/srun_options.rs`
  Additional `srun` resource-flag behavior
- `tests/resource_flags.rs`
  Remaining `sbatch` resource-flag behavior
- `tests/oom.rs`
  Out-of-memory state detection
- `tests/compound_scheduling.rs`
  Compound scheduler scenarios

This file structure can evolve, but the important point is to keep integration
tests organized by behavior, not by source file.

## Current Coverage

The current runtime-oriented test suite covers these user-visible workflows:

- basic `sbatch --wrap` submission through `COMPLETED`
- dependency-gated jobs staying `PENDING` and then completing
- `scontrol show job` core field rendering
- pending-job cancel
- running-job cancel and signal handling
- warning-signal delivery from `sbatch --signal`
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
- `sbatch` output-pattern expansion for `%x`, `%u`, `%N`, and `%%`
- `squeue` filtering, sorting, long view, start view, historical view, and array view
- `sacct` filtering, parsable output, time bounds, and step/allocation record visibility
- `sinfo` partition filtering, long view, node view, custom formats, and `--noheader`
- `squeue -u`
- richer `sacct` field combinations
- `sbatch --export` and `--export-file`
- `sbatch --open-mode append|truncate`
- `sbatch --chdir`
- dependency modes `after`, `afterany`, `afternotok`, and `singleton`
- array stepped and mixed-segment specifications
- allocation-internal `srun` step creation and environment inheritance
- `srun --label`, `--unbuffered`, and clear rejection of `--pty`
- `srun --cpu-bind` success and rejection cases
- `salloc` shell launch without an explicit command
- `salloc` and `srun` runtime reflection for `-J/-p/-n/-G/--mem/-t/-D`
- out-of-memory state detection through cgroup OOM reporting
- `scontrol update` reflection for `Partition` and scheduling impact for `Priority`
- `scontrol update` partition validation against existing constraints
- notification hooks for top-level terminal jobs
- step-target `scancel` references and additional `scancel --signal` variants
- `sbatch` resource-flag reflection and exclusive-host blocking
- compound scheduler flows such as `array + dependency` and `hold + update + release`

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
  - additional `scancel --signal` variants
  - step-target `scancel`
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
  - `sinfo` filtering and rendering behavior
  - `sbatch` export, export-file, open-mode, and chdir behavior
  - `sbatch` output-pattern token expansion
  - allocation-internal `srun` step behavior
  - `salloc` shell launch without an explicit command
  - `salloc` resource flags
  - interactive `srun` options: `--label`, `--unbuffered`, and explicit `--pty`
    rejection
  - `srun --cpu-bind`
  - additional `srun` resource flags
  - OOM state detection
  - `scontrol update` reflection for `Partition` and `Priority`
  - `scontrol update` validation against existing constraints
  - warning-signal delivery from `sbatch --signal`
  - notify-hook behavior
  - notify-hook failure isolation
  - additional `sbatch` resource-flag coverage including exclusive-host behavior
  - compound scheduler scenarios

## Remaining Gaps

The planned runtime suite now covers the main scheduler paths and the primary
user-visible CLI workflows described in this strategy.

What remains is mostly additional depth rather than missing major categories:

- `sbatch`
  - broader flag-combination coverage beyond the current resource, output, dependency, and environment cases
- `srun`
  - more flag/result combinations beyond the current interactive, allocation, cpu-bind, and resource-flag coverage
- `salloc`
  - deeper post-allocation behavior coverage beyond the current shell-launch and resource-flag checks
- `squeue`
  - additional formatting combinations beyond the current filtering, sorting, long/start, array, and `-u` coverage
- `sacct`
  - additional field combinations beyond the current filtering, parsable-output, time-bound, and richer-format coverage
- `scontrol`
  - broader update combinations beyond the currently covered `JobName`, `TimeLimit`, `Priority`, and `Partition` paths
- `scancel`
  - deeper signal/result combinations beyond the current `TERM`/`INT`/`KILL`/`HUP`/`QUIT` and step-target coverage
- `sinfo`
  - deeper formatting combinations beyond the current partition/node/long/custom/noheader coverage

If the current goal is to complete the original runtime-test rollout, this
strategy can be considered substantially complete.

## Recommended Next Pass

The highest-value follow-on additions, if more depth is desired, are:

1. broader `sinfo` and query-surface formatting combinations
2. broader `salloc` / `srun` post-allocation behavior
3. any remaining `sbatch` resource/output combinations
4. deeper `scancel` signal/result combinations

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

The next optional high-value additions are:

1. broader `sinfo` formatting combinations
2. broader `salloc` / `srun` post-allocation behavior
3. any remaining `sbatch` resource/output combinations
4. deeper `scancel --signal` variants and result coverage
