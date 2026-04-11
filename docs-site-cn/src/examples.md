# 示例

## CPU 批处理作业

```bash
sbatch \
  -J hello \
  -p cpu \
  -c 1 \
  --mem 512M \
  -t 00:05:00 \
  -o logs/%j.out \
  --wrap 'echo hello'
```

预期结果：

- `Submitted batch job <id>`
- `logs/<id>.out` 中包含 `hello`

## GPU 批处理作业

```bash
sbatch \
  -J gpu-demo \
  -p gpu \
  -c 4 \
  --mem 8G \
  -G 1 \
  -t 01:00:00 \
  -o logs/%j.out \
  --wrap 'nvidia-smi'
```

预期结果：

- 作业会在 GPU 分区运行
- 输出中包含 `nvidia-smi` 信息

## 交互式前台运行

```bash
srun --label --unbuffered -- echo hello
```

预期结果：

```text
0: hello
```

## 交互式 allocation

```bash
salloc -p gpu -c 4 --mem 8G -G 1 -t 00:30:00
```

预期结果：

- `Granted job allocation <id>`
- 在 allocation 内启动一个 shell

## Array Job

```bash
sbatch \
  -J array-demo \
  -a 0-9%2 \
  -o logs/%A_%a.out \
  --wrap 'echo task=$SLURM_ARRAY_TASK_ID'
```

预期结果：

- 会创建多个 task 记录
- 生成类似 `logs/<array_id>_0.out` 的日志文件

## 单次 Requeue

```bash
sbatch --requeue --wrap 'exit 1'
```

预期结果：

- 第一次失败后作业回到 `PENDING`
- 第二次失败后最终状态为 `FAILED`

## 延迟启动

```bash
sbatch --begin now+00:10:00 --wrap 'echo delayed'
```

预期结果：

- 作业会在 begin 时间之前保持 pending
- `squeue --start` 会显示未来的开始时间

## 显式导出环境变量

```bash
sbatch \
  --export FOO=bar,HELLO=world \
  --wrap 'echo "$FOO $HELLO"'
```

预期结果：

- 输出中包含 `bar world`

## 手动运行 daemon

```bash
SLOTD_ROOT="$HOME/.local/share/slotd" ./target/release/slotd daemon
```

然后在另一个 shell 中执行：

```bash
SLOTD_ROOT="$HOME/.local/share/slotd" ./target/release/slotd sbatch --wrap 'echo hello'
```
