# 快速开始

## 1. 验证 daemon

如果你通过脚本安装，并且没有使用 `--skip-systemd`，daemon 应该已经启动。

先检查基础命令：

```bash
sinfo
squeue
sacct
```

首次运行时的典型结果：

- `sinfo` 会为每个已配置分区显示一行
- `squeue` 为空
- `sacct` 为空

## 2. 提交一个简单的批处理作业

```bash
sbatch --wrap 'echo hello from slotd'
```

典型输出：

```text
Submitted batch job 1
```

## 3. 查看队列

```bash
squeue
```

作业运行时的典型输出：

```text
JOBID | PARTITION | NAME | USER | ST | TIME | NODELIST(REASON)
1     | cpu       | wrap | ...  | R  | 0:00 | localhost
```

## 4. 查看已完成作业

```bash
sacct
```

作业完成后的典型输出：

```text
JobID | Partition | JobName | User | State     | ExitCode
1     | cpu       | wrap    | ...  | COMPLETED | 0:0
```

## 5. 查看详细作业信息

```bash
scontrol show job 1
```

这里会显示：

- 作业标识与所有者
- 作业状态与 reason
- 请求的资源
- 输出路径
- 工作目录
- 各类时间戳

## 6. 试一下交互执行

```bash
srun --label --unbuffered -- echo hello
```

典型输出：

```text
0: hello
```
