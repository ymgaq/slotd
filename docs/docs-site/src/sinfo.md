# Node and Partition View

## `sinfo`

`sinfo` shows partition and host state for the local node.

### Common Options

| Option | Meaning |
| --- | --- |
| `-p`, `--partition` | Filter partitions |
| `-N`, `--Node` | Switch to the single-node summary view |
| `-l`, `--long` | Use the long default view |
| `-o`, `--format` | Select output fields |
| `--noheader` | Omit the header |

## Default View

Typical output:

```text
PARTITION | HOSTNAMES | STATE | FEATURES | GRES_USED
cpu*      | localhost | idle  | cpu      | N/A
gpu       | localhost | idle  | cpu,generic_gpu | gpu:0
```

Notes:

- one row is shown per configured partition
- the default partition is marked with `*`
- CPU partitions show only `cpu` in `FEATURES`
- GPU partitions show `cpu` plus detected GPU model features, and do not include the generic `gpu` label in `FEATURES`
- CPU and GPU partitions are virtual views for convenience on the same host
- CPU and memory capacity are shared across those partitions rather than split into separate pools

## Node View

`sinfo -N` collapses the partition rows into a single local node summary.

Typical output:

```text
PARTITION | HOSTNAMES | STATE
cpu,gpu*  | localhost | idle
```

Notes:

- the `PARTITION` column becomes a comma-joined partition list
- the default partition in that list keeps the `*` marker
- capacity and allocation fields in long or formatted node views are aggregated across the visible partitions

## Long View

`sinfo -l` adds capacity and allocation details:

```text
PARTITION | HOSTNAMES | STATE | FEATURES | CPUS | CPU_ALLOC | MEMORY | MEM_ALLOC | GPUS | GPU_ALLOC | RUNNING | PENDING | GRES_USED
```

## Supported Format Fields

Field names:

- `Partition`
- `Hostnames`, `Hostname`, `NodeList`
- `State`
- `Features`
- `CPUS`
- `CPU_ALLOC`, `CPUSLOAD`, `CPUALLOC`
- `Memory`, `Mem`
- `MEM_ALLOC`, `MemoryAllocated`, `MemAlloc`
- `GPUS`
- `GPU_ALLOC`, `GpusAllocated`, `GpuAlloc`
- `Running`, `RunningJobs`
- `Pending`, `PendingJobs`
- `GRES_USED`, `GresUsed`

Supported `%` codes:

- `%P`
- `%N`
- `%t`, `%T`
- `%f`
- `%G`
