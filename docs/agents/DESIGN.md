# slotd Design Overview

## Purpose

This document describes the active design direction for `slotd`.

It is not the source of truth for what is already implemented. For current
behavior, command coverage, and known simplifications that exist in the code
today, see:

- [IMPLEMENTED.md](/home/yu_yamaguchi/workspace/slotd/docs/agents/IMPLEMENTED.md)
- [COMMAND_REFERENCE.md](/home/yu_yamaguchi/workspace/slotd/docs/agents/COMMAND_REFERENCE.md)

This file should answer a different question:

- what kind of system `slotd` is trying to become
- which gaps matter most
- in what order they should be addressed

## Product Direction

`slotd` should be understandable as a practical single-node subset of Slurm.

That means:

- preserve familiar Slurm command names
- preserve familiar Slurm option names where practical
- preserve familiar user-visible behavior where the single-node scope permits it
- keep internal implementation small, local, and maintainable
- document simplifications explicitly when exact Slurm behavior is out of scope

`slotd` is intentionally not:

- a multi-node scheduler
- a controller / worker cluster stack
- a fairshare, QoS, or accounting management system
- a full reproduction of upstream Slurm internals

## Stable Design Constraints

The following constraints are not temporary roadmap items. They are part of the
intended shape of the project.

### Single Host

`slotd` schedules one local machine.

Implications:

- one daemon owns scheduling and execution
- one local SQLite database is the durable state store
- one host provides the runnable resources
- no distributed launch protocol is required

### One Binary, Slurm-Named Entrypoints

`slotd` should remain one Rust binary with `argv[0]` dispatch and Slurm-style
aliases such as `sbatch`, `srun`, `salloc`, `squeue`, `sacct`, `scancel`,
`scontrol`, and `sinfo`.

### Local Persistence and Recovery

SQLite-backed durable state remains the right tradeoff for:

- restart recovery
- stable queue and accounting queries
- a simple local install model

### Internal Simplicity, External Familiarity

Implementation shortcuts are acceptable when they reduce complexity without
breaking the user's mental model.

They are not acceptable when they cause a supported option to mean something
substantially different from Slurm without being clearly documented.

## Design Rules

When choosing between two implementation approaches, prefer the one that
improves user-visible semantic correctness first.

Priority order:

1. make supported flags mean the right thing
2. make resource defaults and reporting reflect the host accurately
3. make runtime enforcement explicit and reliable
4. only then expand secondary compatibility surface

Corollaries:

- a flag that is accepted but ignored is worse than a flag that is rejected with
  a clear message
- a simplified implementation is fine if its simplification is documented and
  observable behavior stays coherent
- docs must move with code in the same step whenever user-visible behavior
  changes

## Current Design Gaps

The highest-value remaining gaps are these:

1. intentionally simplified semantics should be documented as such
2. queue-estimation and placement semantics are still much simpler than Slurm
3. optional runtime isolation should become more observable to operators
4. secondary compatibility surface should expand only when semantics stay honest

These are more important than adding new commands or broadening rarely used
option coverage.

## Active Development Plan

Development should proceed in small implementation steps. A pull-request-based
breakdown is not required; the unit of work is the step itself.

Each step should update:

- code
- tests
- user-facing docs when behavior changes
- internal docs when design intent or current truth changes

### Step 1: Test Maintenance Refactor

Goal:

- improve test maintainability and readability without changing test logic or
  observable behavior

Rules:

- keep CLI invocations visible in each test instead of hiding scenarios behind a
  large DSL
- prefer thin helper APIs in `tests/helpers/mod.rs`
- split oversized test files by topic before adding more abstraction
- stop once readability improves materially; do not refactor for uniformity
  alone

Planned sequence:

1. classify repeated test patterns and separate "good duplication" from helper
   candidates
2. keep `tests/helpers/mod.rs` focused on runtime setup, common job submission,
   state waiting, and repeated assertion helpers
3. add only a minimal helper set such as:
   - `submit_batch(...) -> job_id`
   - `submit_pending_afterok(...) -> (blocker_id, job_id)`
   - `assert_job_details_contains(job_id, &[...])`
4. migrate the highest-value repetition first, starting with `tests/update.rs`
5. split very large files such as `tests/query_output.rs` by command or concern
   before adding more helper surface
6. apply the same thin-helper approach to other files only where repetition is
   clearly structural, not scenario-specific
7. run focused test targets after each step rather than doing one large
   refactor pass

Explicit non-goals:

- no broad test DSL
- no macro-heavy test generation for integration flows
- no abstraction that hides which Slurm-like command is being exercised
- no repository-wide churn just to normalize style

Completion criteria:

- the largest test files become easier to scan
- repeated setup and detail assertions shrink
- failure output remains at least as clear as before
- helper code stays smaller and simpler than the scenarios it supports

### Step 2: Documented Simplifications

Goal:

- keep the simplified single-node model, but describe it precisely where it
  diverges from upstream Slurm

Initial targets:

- `--constraint`
- `squeue --start`
- any remaining reservation-only behavior

These areas do not need immediate feature expansion if the current semantics are
explicit, tested, and coherent.

### Step 3: Better Operator Visibility

Goal:

- make the scheduler's simplified runtime model easier to inspect and reason
  about during troubleshooting

Initial targets:

- clearer surfacing of whether runtime enforcement is active or reservation-only
- more explicit reporting around aggregated single-node state
- focused troubleshooting notes for cgroup and resource-detection behavior

This step improves operability without expanding the core model.

### Step 4: Selective Compatibility Expansion

Goal:

- expand compatibility only where the single-node model can support semantics
  that are still honest and maintainable

Rule:

- prefer implementing fields and views before adding more accepted flags
- do not accept a flag unless it has either meaningful behavior or a deliberate,
  documented rejection path

## Documentation Policy

`slotd` has three documentation layers, and they should not drift apart.

### Internal Design Docs

Files under `docs/agents/` are for maintainers and contributors.

Rules:

- `DESIGN.md` contains target direction and ordered development priorities
- `IMPLEMENTED.md` contains current repository truth
- completed roadmap items should be removed from `DESIGN.md` or rewritten as
  stable design constraints
- detailed command behavior belongs in `COMMAND_REFERENCE.md`, not here

### User-Facing Docs

User-facing docs are:

- [README.md](/home/yu_yamaguchi/workspace/slotd/README.md)
- `docs/docs-site/`
- `docs/docs-site-ja/`
- `docs/docs-site-cn/`

Rules:

- if a command's visible behavior changes, the corresponding user docs must be
  updated in the same step
- if behavior stays simplified, docs should describe the simplification rather
  than implying full Slurm compatibility
- translated docs should be kept aligned closely enough that they do not convey
  contradictory behavior

## Testing Policy for Roadmap Work

Each roadmap step should add tests that validate observable behavior.

Expected bias:

- unit tests for pure parsing, formatting, and helper logic
- integration tests in `tests/` for daemon, CLI, scheduler, and runtime behavior
- avoid documenting a new behavior until it is covered by tests

For these active roadmap items in particular:

- test-maintenance work should preserve or improve focused integration coverage
  for the files it touches
- documented simplifications should gain tests where current behavior would
  otherwise remain ambiguous
- operator-visibility work needs tests that prove the surfaced state is coherent
- interface cleanup needs tests that prove either real behavior or explicit
  rejection

## What No Longer Belongs in This File

The following content should not be reintroduced here unless it becomes active
design work again:

- lists of commands that are already implemented
- job states that already exist in the code
- fields that already exist in the persisted model
- old phase-zero or MVP transition plans that have already been completed
- outdated claims such as "`srun` is still asynchronous-only" or "multiple
  partitions are not yet modeled" once those are no longer true in the current
  tree

Those belong in `IMPLEMENTED.md` if they describe the present system, or should
be deleted if they only describe old history.

## Decision Standard

Before adding a new feature, ask:

1. does it close a semantic gap in already-supported Slurm surface area
2. does it improve correctness of current reporting or enforcement
3. does it reduce an interface lie or documentation mismatch

If the answer to all three is no, it is probably lower priority than the active
steps above.
