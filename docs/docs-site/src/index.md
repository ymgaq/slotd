# slotd

`slotd` is a Rust-based, single-node, single-user scheduler with a Slurm-style command surface.

Other languages:
- [日本語](https://ymgaq.github.io/slotd/ja/)
- [中文版](https://ymgaq.github.io/slotd/cn/)

It is intended for one workstation, not for a cluster. The goal is to keep common Slurm command names and familiar options while simplifying the runtime model:

- one local daemon
- one SQLite database
- one execution host
- one local user workflow

You use `slotd` through the same command names you would expect in Slurm:

- `sbatch`
- `srun`
- `salloc`
- `squeue`
- `sacct`
- `scontrol`
- `scancel`
- `sinfo`

## What It Is Good For

`slotd` works well for:

- local experiment queues
- long-running CPU or GPU jobs
- one-machine batch pipelines
- interactive work with resource reservation
- a lightweight Slurm-like interface on a workstation

It is not trying to provide:

- multi-node scheduling
- cluster administration
- accounts, QoS, or fairshare
- federation or reservations across hosts

## Main Characteristics

- Built as a single Rust binary
- Uses a daemon plus a Unix domain socket
- Persists state in SQLite
- Schedules CPU, memory, and GPU reservations
- Supports batch jobs, arrays, interactive runs, allocations, and steps
- Supports delayed start, requeue-once, dependencies, and local feature constraints

## Documentation Map

- [Installation](installation.md)
- [Quick Start](quickstart.md)
- [Runtime Model](runtime-model.md)
- [Batch Jobs with `sbatch`](sbatch.md)
- [Interactive Execution with `srun`](srun.md)
- [Allocations with `salloc`](salloc.md)
- [Queue and Accounting](queue-and-accounting.md)
- [Job Control](job-control.md)
- [Node and Partition View](sinfo.md)
- [Examples](examples.md)
- [Troubleshooting](troubleshooting.md)
