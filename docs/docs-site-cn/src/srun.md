# `srun` 交互执行

## 形式

```bash
srun [options] -- <command...>
```

## `srun` 的行为

`srun` 默认以前台方式运行命令。

行为取决于你是否已经位于某个 allocation 内部：

- 在 allocation 内部：
  - 创建一个 step 记录
  - 直接前台运行命令
- 在 allocation 外部：
  - 创建一个类似 allocation 的顶层记录
  - 等待其可以运行
  - 创建一个 step 记录
  - 前台运行命令

只有 `--no-wait` 会提交一个由 daemon 管理的 run job。

当 `--ntasks` 大于 `1` 时，前台 `srun` 会在同一台主机上按 task rank 启动一个
本地进程，并导出 `SLURM_PROCID` 与 `SLURM_LOCALID`。

## 主要选项

| 选项 | 含义 |
| --- | --- |
| `-J`, `--job-name <name>` | 设置作业名 |
| `-p`, `--partition <partition>` | 选择分区 |
| `-c`, `--cpus-per-task <n>` | 每个 task 的 CPU 数 |
| `-n`, `--ntasks <n>` | 并发启动的本地 task 数量 |
| `--mem <size>` | 请求内存 |
| `-t`, `--time <time>` | 时间限制 |
| `-G`, `--gpus <n>` | 请求 GPU slot 数 |
| `-o`, `--output <path>` | 前台 stdout 输出路径 |
| `-e`, `--error <path>` | 前台 stderr 输出路径 |
| `-D`, `--chdir <path>` | 工作目录 |
| `--immediate` | 如果资源不能立即可用则直接失败 |
| `--pty` | 选择前台执行路径 |
| `--constraint <feature>` | 要求匹配的本地 feature |
| `--cpu-bind <mode>` | 设置 CPU affinity |
| `--label` | 在输出前加上 `<task_id>: ` 前缀 |
| `--unbuffered` | 积极 flush 转发输出 |
| `--no-wait` | 提交 daemon 管理的 run job |

## 输出行为

示例：

```bash
srun --label --unbuffered -- echo hello
```

典型输出：

```text
0: hello
```

## CPU Binding

支持的值：

- `none`
- `cores`
- `map_cpu:<id,id,...>`

示例：

```bash
srun --cpu-bind map_cpu:0,2 -- python train.py
```

## Immediate Mode

`--immediate` 会在资源无法立即获得时直接失败，而不是等待。

示例：

```bash
srun --immediate -p gpu -G 1 -- nvidia-smi
```

## `--no-wait`

`--no-wait` 会把 run job 提交给 daemon，而不是在前台等待。

典型输出：

```text
Submitted run job 12
```

限制：

- `--label` 和 `--unbuffered` 不能与 `--no-wait` 一起使用
