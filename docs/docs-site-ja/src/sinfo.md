# ノードとパーティション表示

## `sinfo`

`sinfo` はローカルノードに対する partition と host state を表示します。

### よく使うオプション

| オプション | 意味 |
| --- | --- |
| `-p`, `--partition` | partition で絞り込む |
| `-N`, `--Node` | 単一ノード要約ビューに切り替える |
| `-l`, `--long` | long default view を使う |
| `-o`, `--format` | 出力 field を選ぶ |
| `--noheader` | header を省略する |

## Default View

典型的な出力:

```text
PARTITION | HOSTNAMES | STATE | FEATURES | GRES_USED
cpu*      | localhost | idle  | cpu      | N/A
gpu       | localhost | idle  | cpu,generic_gpu | gpu:0
```

補足:

- 設定済み partition ごとに 1 行表示される
- default partition には `*` が付く
- CPU partition の `FEATURES` は `cpu` のみを表示する
- GPU partition の `FEATURES` は `cpu` と検出した GPU モデル feature を表示し、汎用の `gpu` は表示しない
- CPU/GPU partition は同じローカルホストを見せ分けるための仮想的な区分
- CPU 容量とメモリ容量は partition 間で共有され、別々の resource pool にはならない

## Node View

`sinfo -N` は partition ごとの行を 1 つのローカルノード要約にまとめます。

典型的な出力:

```text
PARTITION | HOSTNAMES | STATE
cpu,gpu*  | localhost | idle
```

補足:

- `PARTITION` 列は partition 名をカンマで連結した値になる
- その中でも default partition には `*` が付く
- long view や format 指定で出す capacity/allocation 系の値は、表示対象 partition 全体で集約される

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
