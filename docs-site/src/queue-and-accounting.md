# Queue and Accounting

## `squeue`

`squeue` shows top-level queued and running jobs.

### Common Options

| Option | Meaning |
| --- | --- |
| `--all` | Show all states |
| `-t`, `--states` | Filter by state |
| `-j`, `--jobs` | Filter by job IDs |
| `-u`, `--user` | Filter by user |
| `-p`, `--partition` | Filter by partition |
| `-o`, `--format` | Select output fields |
| `-S`, `--sort` | Sort rows |
| `-l`, `--long` | Long default view |
| `--start` | Show estimated start times |
| `--array` | Show array-style job IDs |
| `--noheader` | Omit the header |

### Default View

```text
JOBID | PARTITION | NAME | USER | ST | TIME | NODELIST(REASON)
```

### Long View

```text
JOBID | PARTITION | NAME | USER | ST | TIME | TIME_LIMIT | NTASKS | CPUS | REQ_MEM | REQ_GPU | NODELIST(REASON)
```

### Start-Time View

With `--start` and no explicit format:

```text
JOBID | PARTITION | NAME | USER | ST | START_TIME | NODELIST(REASON)
```

### Format Fields

Supported field names:

- `JobID`
- `Partition`
- `Name`, `JobName`
- `User`
- `ST`, `State`
- `Time`, `Elapsed`
- `TimeLimit`, `Time_Limit`
- `NTasks`
- `CPUS`, `ReqCPUS`
- `ReqMem`
- `ReqGPU`, `ReqGPUS`
- `Start`, `StartTime`
- `NodeList(Reason)`, `NodeListReason`, `Reason`, `NodeList`

Supported `%` codes:

- `%i`
- `%P`
- `%j`
- `%u`
- `%t`, `%T`
- `%M`
- `%S`
- `%R`, `%N`

## `sacct`

`sacct` shows persisted accounting data, including completed jobs and steps.

### Common Options

| Option | Meaning |
| --- | --- |
| `-j`, `--jobs` | Filter by job IDs |
| `-s`, `--state` | Filter by state |
| `-S`, `--starttime` | Filter by start time |
| `-E`, `--endtime` | Filter by end time |
| `-u`, `--user` | Filter by user |
| `-p`, `--partition` | Filter by partition |
| `-o`, `--format` | Select output fields |
| `-P`, `--parsable2` | Use `|`-delimited output |
| `-n`, `--noheader` | Omit the header |

### Default View

```text
JobID | Partition | JobName | User | State | ExitCode
```

### Record Types

`sacct` includes:

- top-level jobs
- allocation records
- step records
- completed records

ID rendering rules:

- step IDs appear as `<job_id>.<step_id>`
- array tasks appear as `<array_job_id>_<task_id>`

### Format Fields

Supported field names:

- `JobID`
- `ArrayJobID`
- `ArrayTaskID`
- `JobName`
- `Partition`
- `User`
- `State`
- `Reason`
- `ExitCode`
- `Elapsed`
- `AllocCPUS`
- `ReqMem`
- `ReqTRES`
- `AllocTRES`
- `NodeList`
- `Submit`
- `Start`
- `End`
- `WorkDir`
- `BatchFlag`
- `MaxRSS`

Supported `%` codes:

- `%i`
- `%F`
- `%K`
- `%j`
- `%P`
- `%u`
- `%t`, `%T`
- `%R`
- `%X`
- `%M`
- `%C`
- `%m`
- `%b`
- `%B`
- `%N`
- `%V`
- `%S`
- `%E`
- `%Z`
