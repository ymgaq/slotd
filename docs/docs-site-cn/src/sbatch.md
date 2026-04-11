# `sbatch` 批处理作业

## 形式

```bash
sbatch [options] <script>
sbatch [options] --wrap '<command>'
```

## `sbatch` 的作用

`sbatch` 会创建一个持久化的 batch job 记录，并把它提交给本地 daemon。

在 script 模式下：

- 从磁盘读取脚本
- 将脚本内容保存到 job 目录
- 解析前导 `#SBATCH` 指令

在 `--wrap` 模式下：

- 为命令生成一个内部 shell 脚本

典型输出：

```text
Submitted batch job 1
```

使用 `--parsable` 时：

```text
1
```

## 主要选项

| 选项 | 含义 |
| --- | --- |
| `--wrap <command>` | 提交内联 shell 命令 |
| `-J`, `--job-name <name>` | 设置作业名 |
| `-p`, `--partition <partition>` | 选择分区 |
| `-c`, `--cpus-per-task <n>` | 每个 task 的 CPU 数 |
| `-n`, `--ntasks <n>` | task 数量 |
| `--mem <size>` | 请求内存，例如 `512M` 或 `8G` |
| `-t`, `--time <time>` | 时间限制 |
| `-G`, `--gpus <n>` | 请求 GPU slot 数 |
| `-o`, `--output <path>` | stdout 路径模式 |
| `-e`, `--error <path>` | stderr 路径模式 |
| `-D`, `--chdir <path>` | 工作目录 |
| `--constraint <feature>` | 要求匹配的本地 feature |
| `-d`, `--dependency <spec>` | dependency 表达式 |
| `-a`, `--array <spec>` | array 规格 |
| `--export <spec>` | 向作业环境导出变量 |
| `--export-file <path>` | 从文件加载环境变量 |
| `--open-mode append\|truncate` | 选择追加或截断输出文件 |
| `--signal <spec>` | 在超时前发送 warning signal |
| `--begin <time>` | 延迟作业进入可运行状态 |
| `--exclusive` | 不与其他顶层作业共享主机 |
| `--requeue` | 某些失败状态下只重排一次 |
| `--parsable` | 仅输出 job ID |
| `-W`, `--wait` | 等待作业完成 |

## 默认值

未指定时：

- `cpus-per-task = 1`
- `ntasks = 1`
- `mem = 512M`
- partition = 已配置的默认分区
- GPU 默认值在 GPU 分区为 `1`，否则为 `0`

## `#SBATCH` 支持

支持的指令：

- `-J`, `--job-name`
- `-p`, `--partition`
- `-c`, `--cpus-per-task`
- `-n`, `--ntasks`
- `--mem`
- `-t`, `--time`
- `-G`, `--gpus`
- `-o`, `--output`
- `-e`, `--error`
- `-D`, `--chdir`
- `--constraint`
- `--begin`
- `--exclusive`
- `--requeue`
- `-d`, `--dependency`
- `-a`, `--array`

优先级：

1. 命令行选项
2. `SBATCH_*` 环境变量
3. `#SBATCH` 指令
4. 内建默认值

## Dependencies

支持的 dependency 表达式：

- `after:<jobid>[,<jobid>...]`
- `afterany:<jobid>[,<jobid>...]`
- `afterok:<jobid>[,<jobid>...]`
- `afternotok:<jobid>[,<jobid>...]`
- `singleton`

## Arrays

支持的 array 形式：

- 单个 ID
- 范围，例如 `0-7`
- 带步长的范围，例如 `0-15:2`
- 并发限制，例如 `0-31%4`

示例：

```bash
sbatch -a 0-9%2 --wrap 'echo task=$SLURM_ARRAY_TASK_ID'
```

预期结果：

- 会持久化多个 task 记录
- 同一 array 同时最多运行两个 task

## Delayed Start

`--begin` 支持：

- epoch 秒
- `YYYY-MM-DD`
- `YYYY-MM-DDTHH:MM:SS`
- `now+<duration>`

示例：

```bash
sbatch --begin now+00:10:00 --wrap 'echo delayed'
```

## Requeue Once

`--requeue` 改变失败后的处理方式：

- `FAILED` 只重排一次
- `TIMEOUT` 只重排一次
- `OUT_OF_MEMORY` 只重排一次
- `COMPLETED` 不重排
- `CANCELLED` 不重排

示例：

```bash
sbatch --requeue --wrap 'exit 1'
```

## Output Paths

路径模式中的 token：

- `%j`: job ID
- `%A`: array job ID
- `%a`: array task ID
- `%x`: job name
- `%u`: user name
- `%N`: hostname
- `%%`: 字面 `%`

默认值：

- 非 array 的 stdout: `slurm-%j.out`
- array 的 stdout: `slurm-%A_%a.out`
- 如果未设置 `--error`，stderr 默认与 stdout 相同

## Environment Export

`--export` 支持：

- `ALL`
- `NONE`
- `KEY=VALUE,...`

示例：

```bash
sbatch --export FOO=bar,HELLO=world --wrap 'echo "$FOO $HELLO"'
```

预期结果：

- 输出中包含 `bar world`
