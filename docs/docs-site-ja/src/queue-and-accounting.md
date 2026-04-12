# キュー状態と集計情報

## `squeue`

`squeue` は top-level の queued job と running job を表示します。

### よく使うオプション

| オプション | 意味 |
| --- | --- |
| `--all` | すべての状態を表示する |
| `-t`, `--states` | 状態で絞り込む |
| `-j`, `--jobs` | job ID で絞り込む |
| `-u`, `--user` | user で絞り込む |
| `-p`, `--partition` | partition で絞り込む |
| `-o`, `--format` | 出力 field を選ぶ |
| `-S`, `--sort` | 行の並び順を指定する |
| `-l`, `--long` | long default view を使う |
| `--start` | 推定開始時刻を表示する |
| `--array` | array 形式の job ID を表示する |
| `--noheader` | header を省略する |

### Default View

```text
JOBID | PARTITION | NAME | USER | ST | TIME | NODELIST(REASON)
```

### Long View

```text
JOBID | PARTITION | NAME | USER | ST | TIME | TIME_LIMIT | NTASKS | CPUS | REQ_MEM | REQ_GPU | NODELIST(REASON)
```

### Start-Time View

`--start` を付け、format を明示しない場合:

```text
JOBID | PARTITION | NAME | USER | ST | START_TIME | NODELIST(REASON)
```

### Format Fields

サポートする field 名:

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

サポートする `%` code:

- `%i`
- `%P`
- `%j`
- `%u`
- `%t`, `%T`
- `%M`
- `%S`
- `%R`, `%N`

## `sacct`

`sacct` は完了済み job や step を含む永続化済み accounting data を表示します。

### よく使うオプション

| オプション | 意味 |
| --- | --- |
| `-j`, `--jobs` | job ID で絞り込む |
| `-s`, `--state` | 状態で絞り込む |
| `-S`, `--starttime` | 開始時刻で絞り込む |
| `-E`, `--endtime` | 終了時刻で絞り込む |
| `-u`, `--user` | user で絞り込む |
| `-p`, `--partition` | partition で絞り込む |
| `-o`, `--format` | 出力 field を選ぶ |
| `-P`, `--parsable2` | `|` 区切り出力にする |
| `-n`, `--noheader` | header を省略する |

### Default View

```text
JobID | Partition | JobName | User | State | ExitCode
```

### Record Types

`sacct` に含まれる record:

- top-level job
- allocation record
- step record
- completed record

ID の表示ルール:

- step ID は `<job_id>.<step_id>`
- array task は `<array_job_id>_<task_id>`

### Format Fields

サポートする field 名:

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

サポートする `%` code:

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
