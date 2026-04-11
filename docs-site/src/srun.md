# Interactive Execution with `srun`

## Form

```bash
srun [options] -- <command...>
```

## What `srun` Does

`srun` runs a command in the foreground by default.

Behavior depends on whether you are already inside an allocation:

- inside an allocation:
  - creates a step record
  - runs the command directly in the foreground
- outside an allocation:
  - creates an allocation-like top-level record
  - waits for it to run
  - creates a step record
  - runs the command in the foreground

Only `--no-wait` submits a daemon-managed run job.

## Main Options

| Option | Meaning |
| --- | --- |
| `-J`, `--job-name <name>` | Set the job name |
| `-p`, `--partition <partition>` | Choose a partition |
| `-c`, `--cpus-per-task <n>` | CPUs per task |
| `-n`, `--ntasks <n>` | Number of tasks |
| `--mem <size>` | Requested memory |
| `-t`, `--time <time>` | Time limit |
| `-G`, `--gpus <n>` | Requested GPU slots |
| `-o`, `--output <path>` | Foreground stdout path |
| `-e`, `--error <path>` | Foreground stderr path |
| `-D`, `--chdir <path>` | Working directory |
| `--immediate` | Fail if resources are not available immediately |
| `--pty` | Select the foreground execution path |
| `--constraint <feature>` | Require matching local features |
| `--cpu-bind <mode>` | Bind CPU affinity |
| `--label` | Prefix forwarded output with `0: ` |
| `--unbuffered` | Flush forwarded output eagerly |
| `--no-wait` | Submit a daemon-managed run job |

## Output Behavior

Example:

```bash
srun --label --unbuffered -- echo hello
```

Typical output:

```text
0: hello
```

## CPU Binding

Supported values:

- `none`
- `cores`
- `map_cpu:<id,id,...>`

Example:

```bash
srun --cpu-bind map_cpu:0,2 -- python train.py
```

## Immediate Mode

`--immediate` fails instead of waiting if resources are not available right away.

Example:

```bash
srun --immediate -p gpu -G 1 -- nvidia-smi
```

## `--no-wait`

`--no-wait` submits a run job to the daemon instead of waiting in the foreground.

Typical output:

```text
Submitted run job 12
```

Restrictions:

- `--label` and `--unbuffered` are not supported together with `--no-wait`
