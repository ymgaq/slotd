# Node and Partition View

## `sinfo`

`sinfo` shows partition and host state for the local node.

### Common Options

| Option | Meaning |
| --- | --- |
| `-p`, `--partition` | Filter partitions |
| `-N`, `--Node` | Accepted for compatibility |
| `-l`, `--long` | Use the long default view |
| `-o`, `--format` | Select output fields |
| `--noheader` | Omit the header |

## Default View

Typical output:

```text
PARTITION | HOSTNAMES | STATE | FEATURES | GRES_USED
cpu*      | localhost | idle  | cpu      | N/A
gpu       | localhost | idle  | cpu,gpu  | gpu:0
```

Notes:

- one row is shown per configured partition
- the default partition is marked with `*`

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
