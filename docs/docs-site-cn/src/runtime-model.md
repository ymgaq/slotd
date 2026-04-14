# 运行模型

## 高层模型

`slotd` 是一个单主机调度器。它的运行模型刻意保持简单：

- 一个本地 daemon
- 一个本地 SQLite 数据库
- 一个本地执行主机
- 一个本地用户工作流

没有 controller/worker 拆分，也没有远程节点启动协议。

## 核心资源

`slotd` 调度三类资源：

- CPU
- 内存
- GPU

当前行为：

- CPU 预留量等于 `ntasks * cpus-per-task`
- `ntasks` 会在 batch 和前台执行中按 task rank 启动一个本地进程
- 总内存默认从 `/proc/meminfo` 的 `MemTotal` 检测，失败时回退到 `16384 MB`
- 内存以 MB 保存
- GPU 以整数 slot 表示
- 准入基于预留量，而不是实际使用量
- 如果 `SLOTD_CGROUP_BASE` 未设置，CPU 和内存仍然只是预留值
- 如果 `SLOTD_CGROUP_BASE` 指向可写的 cgroup v2 subtree，`slotd` 会写入 `memory.max` 与 `cpu.max`
- 显式启用后如果 cgroup 设置失败，作业启动会失败，而不是静默跳过 enforcement

## 分区

通过环境变量配置：

- `SLOTD_CPU_PARTITIONS`
- `SLOTD_GPU_PARTITIONS`

规则：

- 只接受已配置的分区名称
- 如果没有 GPU，则不会暴露 GPU 分区
- 如果选择了 GPU 分区且省略 `--gpus`，默认 GPU 请求为 `1`
- 否则默认 GPU 请求为 `0`
- CPU/GPU 分区是为了使用方便而划分的同一台本地主机的虚拟视图
- CPU 和内存容量在分区之间共享，分区差异主要体现在 GPU 可见性和默认值

## GPU 检测

如果没有设置 `SLOTD_GPU_COUNT`，`slotd` 会尝试通过 `nvidia-smi` 检测 GPU。

当前实现会检查：

- `nvidia-smi`
- `/usr/bin/nvidia-smi`
- `/usr/lib/wsl/lib/nvidia-smi`
- `/bin/nvidia-smi`

## 作业类型

持久化记录分为以下几类：

- 顶层 batch job
- 仅 allocation 的 job
- array task
- allocation 下的 step

## 作业状态

已实现的状态：

- `PENDING`
- `RUNNING`
- `COMPLETING`
- `COMPLETED`
- `FAILED`
- `CANCELLED`
- `TIMEOUT`
- `OUT_OF_MEMORY`

终态：

- `COMPLETED`
- `FAILED`
- `CANCELLED`
- `TIMEOUT`
- `OUT_OF_MEMORY`

## 调度规则

daemon 循环每 `300ms` 运行一次。

pending job 可能被以下条件阻塞：

- dependency
- array concurrency limit
- delayed start time
- exclusive host use
- 预留资源不足
- user hold state

排序规则：

- 基础规则是提交顺序
- 显式 job priority 可以覆盖纯提交顺序
- array task 会按 array group 交错执行

## Runtime 文件

在 `SLOTD_ROOT` 下：

- `run/slotd.sock`: daemon socket
- `lib/state.db`: SQLite 状态库
- `lib/jobs/<job_id>/script.sh`: batch 脚本
- `lib/jobs/<job_id>/runner.sh`: daemon wrapper
- `lib/jobs/<job_id>/exit_status`: wrapper 退出状态

## 通知

如果设置了 `SLOTD_NOTIFY_CMD`，`slotd` 会在顶层 job 进入终态时执行它。

导出的变量：

- `SLOTD_JOB_ID`
- `SLOTD_JOB_NAME`
- `SLOTD_JOB_STATE`
- `SLOTD_JOB_PARTITION`
- `SLOTD_JOB_REASON`
