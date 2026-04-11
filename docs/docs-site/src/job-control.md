# Job Control

## `scontrol`

Supported forms:

```bash
scontrol show job <job_id>
scontrol hold job <job_id>
scontrol release job <job_id>
scontrol update job <job_id> KEY=VALUE...
```

### `show job`

Shows detailed job information, including:

- job identity and ownership
- state and reason
- requested resources
- time limits
- dependency string
- submit, start, and end timestamps
- command and working directory
- stdout and stderr paths
- array metadata
- `ReqTRES`
- `AllocTRES`
- `MaxRSS`
- step summary

### `hold job`

Moves a pending job into a held state.

Result:

- the job stays `PENDING`
- the reason becomes `JobHeldUser`

### `release job`

Releases a held pending job back into normal scheduling.

### `update job`

Supported update keys:

| Key | Rule |
| --- | --- |
| `JobName` / `Name` | Can be changed only while the job is `PENDING` |
| `Partition` | Can be changed only while the job is `PENDING` |
| `TimeLimit` / `Time` | Can be changed until the job reaches a terminal state |
| `Priority` | Can be changed only while the job is `PENDING` |

Example:

```bash
scontrol update job 10 TimeLimit=02:00:00
```

## `scancel`

Supported forms:

```bash
scancel <job_id>
scancel <job_id.step_id>
scancel --signal <sig> <job_id>
scancel --signal <sig> <job_id.step_id>
```

### Default Cancel Behavior

- pending jobs become `CANCELLED` immediately
- running jobs transition through `COMPLETING`
- the runner sends `SIGTERM`
- after the grace period it sends `SIGKILL` if needed

The recorded cancel reason is:

```text
CancelledByUser
```

### Signal Mode

`--signal` sends a specific signal instead of performing normal cancellation.

Example:

```bash
scancel --signal TERM 12
```
