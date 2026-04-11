# ノードとパーティション表示

## `sinfo`

`sinfo` はローカルノードに対する partition と host state を表示します。

### よく使うオプション

| オプション | 意味 |
| --- | --- |
| `-p`, `--partition` | partition で絞り込む |
| `-N`, `--Node` | 互換性のために受理する |
| `-l`, `--long` | long default view を使う |
| `-o`, `--format` | 出力 field を選ぶ |
| `--noheader` | header を省略する |

## Default View

典型的な出力:

```text
PARTITION | HOSTNAMES | STATE | FEATURES | GRES_USED
cpu*      | localhost | idle  | cpu      | N/A
gpu       | localhost | idle  | cpu,gpu  | gpu:0
```

補足:

- 設定済み partition ごとに 1 行表示される
- default partition には `*` が付く

## Long View

`sinfo -l` では capacity と allocation の詳細が追加されます。

```text
PARTITION | HOSTNAMES | STATE | FEATURES | CPUS | CPU_ALLOC | MEMORY | MEM_ALLOC | GPUS | GPU_ALLOC | RUNNING | PENDING | GRES_USED
```

## Supported Format Fields

field 名:

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

サポートする `%` code:

- `%P`
- `%N`
- `%t`, `%T`
- `%f`
- `%G`
