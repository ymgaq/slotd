# 节点与分区视图

## `sinfo`

`sinfo` 显示本地节点的分区与主机状态。

### 常用选项

| 选项 | 含义 |
| --- | --- |
| `-p`, `--partition` | 过滤分区 |
| `-N`, `--Node` | 切换到单节点汇总视图 |
| `-l`, `--long` | 使用长格式默认视图 |
| `-o`, `--format` | 选择输出字段 |
| `--noheader` | 省略表头 |

## 默认视图

典型输出：

```text
PARTITION | HOSTNAMES | STATE | FEATURES | GRES_USED
cpu*      | localhost | idle  | cpu      | N/A
gpu       | localhost | idle  | cpu,generic_gpu | gpu:0
```

说明：

- 每个已配置分区显示一行
- 默认分区会标记为 `*`
- CPU partition 的 `FEATURES` 只显示 `cpu`
- GPU partition 的 `FEATURES` 显示 `cpu` 和检测到的 GPU 型号 feature，不显示通用 `gpu`
- CPU/GPU 分区只是同一台本地主机上的便捷虚拟视图
- CPU 和内存容量在这些分区之间共享，并不是彼此独立的资源池

## 节点视图

`sinfo -N` 会把分区行折叠为单个本地节点汇总行。

典型输出：

```text
PARTITION | HOSTNAMES | STATE
cpu,gpu*  | localhost | idle
```

说明：

- `PARTITION` 列会变成用逗号连接的分区列表
- 其中默认分区仍然保留 `*` 标记
- 长视图或自定义格式中的容量和分配字段会按可见分区聚合

## 长视图

`sinfo -l` 会增加容量与分配情况的细节：

```text
PARTITION | HOSTNAMES | STATE | FEATURES | CPUS | CPU_ALLOC | MEMORY | MEM_ALLOC | GPUS | GPU_ALLOC | RUNNING | PENDING | GRES_USED
```

## 支持的格式字段

字段名：

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

支持的 `%` 代码：

- `%P`
- `%N`
- `%t`, `%T`
- `%f`
- `%G`
