# Quick Start

## 1. Verify the Daemon

If you installed with the script and did not use `--skip-systemd`, the daemon should already be running.

Check the basic commands:

```bash
sinfo
squeue
sacct
```

Typical first-run output:

- `sinfo` shows one row per configured partition
- `squeue` is empty
- `sacct` is empty

## 2. Submit a Simple Batch Job

```bash
sbatch --wrap 'echo hello from slotd'
```

Typical output:

```text
Submitted batch job 1
```

## 3. Inspect the Queue

```bash
squeue
```

Typical output while a job is active:

```text
JOBID | PARTITION | NAME | USER | ST | TIME | NODELIST(REASON)
1     | cpu       | wrap | ...  | R  | 0:00 | localhost
```

## 4. Inspect Completed Jobs

```bash
sacct
```

Typical output after the job finishes:

```text
JobID | Partition | JobName | User | State     | ExitCode
1     | cpu       | wrap    | ...  | COMPLETED | 0:0
```

## 5. Show Detailed Job Information

```bash
scontrol show job 1
```

This shows:

- job identity
- job state and reason
- requested resources
- output paths
- working directory
- timestamps

## 6. Try an Interactive Run

```bash
srun --label --unbuffered -- echo hello
```

Typical output:

```text
0: hello
```
