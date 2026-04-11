# Allocations with `salloc`

## Form

```bash
salloc [options] [command...]
```

## What `salloc` Does

`salloc` creates an allocation-only top-level job, waits for it to become runnable, and then starts a foreground command inside that allocation.

If no command is given, it starts your shell.

Typical output:

```text
Granted job allocation 4
```

## Main Options

| Option | Meaning |
| --- | --- |
| `-J`, `--job-name <name>` | Set the allocation name |
| `-p`, `--partition <partition>` | Choose a partition |
| `-c`, `--cpus-per-task <n>` | CPUs per task |
| `-n`, `--ntasks <n>` | Number of tasks |
| `--mem <size>` | Requested memory |
| `-t`, `--time <time>` | Time limit |
| `-G`, `--gpus <n>` | Requested GPU slots |
| `-D`, `--chdir <path>` | Working directory |
| `--constraint <feature>` | Require matching local features |
| `--immediate` | Fail if the allocation cannot start immediately |

## Example

```bash
salloc -p gpu -c 4 --mem 8G -G 1 -t 00:30:00
```

Expected result:

- an allocation record is created
- the command waits until the allocation is running
- your shell starts inside the allocation
- later `srun` commands become steps under that allocation
