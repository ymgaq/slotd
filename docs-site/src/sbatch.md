# Batch Jobs with `sbatch`

## Forms

```bash
sbatch [options] <script>
sbatch [options] --wrap '<command>'
```

## What `sbatch` Does

`sbatch` creates a persisted batch job record and submits it to the local daemon.

In script mode:

- it reads the script from disk
- stores the body in the job directory
- parses leading `#SBATCH` directives

In `--wrap` mode:

- it creates an internal shell script around the command

Typical output:

```text
Submitted batch job 1
```

With `--parsable`:

```text
1
```

## Main Options

| Option | Meaning |
| --- | --- |
| `--wrap <command>` | Submit an inline shell command |
| `-J`, `--job-name <name>` | Set the job name |
| `-p`, `--partition <partition>` | Choose a partition |
| `-c`, `--cpus-per-task <n>` | CPUs per task |
| `-n`, `--ntasks <n>` | Number of tasks |
| `--mem <size>` | Requested memory, such as `512M` or `8G` |
| `-t`, `--time <time>` | Time limit |
| `-G`, `--gpus <n>` | Requested GPU slots |
| `-o`, `--output <path>` | Stdout path pattern |
| `-e`, `--error <path>` | Stderr path pattern |
| `-D`, `--chdir <path>` | Working directory |
| `--constraint <feature>` | Require matching local features |
| `-d`, `--dependency <spec>` | Dependency expression |
| `-a`, `--array <spec>` | Array specification |
| `--export <spec>` | Export environment values into the job |
| `--export-file <path>` | Load environment variables from a file |
| `--open-mode append\|truncate` | Append to or truncate output files |
| `--signal <spec>` | Send a warning signal before timeout |
| `--begin <time>` | Delay job eligibility |
| `--exclusive` | Do not share the host with other top-level jobs |
| `--requeue` | Requeue once after certain failure states |
| `--parsable` | Print only the job ID |
| `-W`, `--wait` | Wait for job completion |

## Defaults

When not specified:

- `cpus-per-task = 1`
- `ntasks = 1`
- `mem = 512M`
- partition = configured default partition
- GPUs default to `1` for GPU partitions and `0` otherwise

## `#SBATCH` Support

Supported directives:

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

Precedence:

1. command-line options
2. `SBATCH_*` environment variables
3. `#SBATCH` directives
4. built-in defaults

## Dependencies

Supported dependency expressions:

- `after:<jobid>[,<jobid>...]`
- `afterany:<jobid>[,<jobid>...]`
- `afterok:<jobid>[,<jobid>...]`
- `afternotok:<jobid>[,<jobid>...]`
- `singleton`

## Arrays

Supported array forms:

- single IDs
- ranges, such as `0-7`
- stepped ranges, such as `0-15:2`
- concurrency limits, such as `0-31%4`

Example:

```bash
sbatch -a 0-9%2 --wrap 'echo task=$SLURM_ARRAY_TASK_ID'
```

Expected result:

- multiple persisted task records
- at most two running at the same time for that array

## Delayed Start

`--begin` supports:

- epoch seconds
- `YYYY-MM-DD`
- `YYYY-MM-DDTHH:MM:SS`
- `now+<duration>`

Example:

```bash
sbatch --begin now+00:10:00 --wrap 'echo delayed'
```

## Requeue Once

`--requeue` changes failure handling:

- `FAILED` requeues once
- `TIMEOUT` requeues once
- `OUT_OF_MEMORY` requeues once
- `COMPLETED` does not requeue
- `CANCELLED` does not requeue

Example:

```bash
sbatch --requeue --wrap 'exit 1'
```

## Output Paths

Pattern tokens:

- `%j`: job ID
- `%A`: array job ID
- `%a`: array task ID
- `%x`: job name
- `%u`: user name
- `%N`: hostname
- `%%`: literal `%`

Defaults:

- non-array stdout: `slurm-%j.out`
- array stdout: `slurm-%A_%a.out`
- stderr defaults to stdout unless `--error` is set

## Environment Export

`--export` supports:

- `ALL`
- `NONE`
- `KEY=VALUE,...`

Example:

```bash
sbatch --export FOO=bar,HELLO=world --wrap 'echo "$FOO $HELLO"'
```

Expected result:

- the output contains `bar world`
