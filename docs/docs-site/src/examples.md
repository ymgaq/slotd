# Examples

## CPU Batch Job

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

Expected result:

- `Submitted batch job <id>`
- `logs/<id>.out` contains `hello`

## GPU Batch Job

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

Expected result:

- the job runs on the GPU partition
- output contains `nvidia-smi` data

## Interactive Foreground Run

```bash
srun --label --unbuffered -- echo hello
```

Expected result:

```text
0: hello
```

## Interactive Allocation

```bash
salloc -p gpu -c 4 --mem 8G -G 1 -t 00:30:00
```

Expected result:

- `Granted job allocation <id>`
- a shell starts inside the allocation

## Array Job

```bash
sbatch \
  -J array-demo \
  -a 0-9%2 \
  -o logs/%A_%a.out \
  --wrap 'echo task=$SLURM_ARRAY_TASK_ID'
```

Expected result:

- multiple task records
- logs such as `logs/<array_id>_0.out`

## Requeue Once

```bash
sbatch --requeue --wrap 'exit 1'
```

Expected result:

- the first failure returns the job to `PENDING`
- the second failure leaves the final state as `FAILED`

## Delayed Start

```bash
sbatch --begin now+00:10:00 --wrap 'echo delayed'
```

Expected result:

- the job remains pending until the begin time
- `squeue --start` shows an estimated future start time

## Explicit Export

```bash
sbatch \
  --export FOO=bar,HELLO=world \
  --wrap 'echo "$FOO $HELLO"'
```

Expected result:

- output contains `bar world`

## Manual Daemon Run

```bash
SLOTD_ROOT="$HOME/.local/share/slotd" ./target/release/slotd daemon
```

Then from another shell:

```bash
SLOTD_ROOT="$HOME/.local/share/slotd" ./target/release/slotd sbatch --wrap 'echo hello'
```
