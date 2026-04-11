# 队列与记账信息

## `squeue`

`squeue` 显示顶层排队作业和运行中的作业。

### 常用选项

| 选项 | 含义 |
| --- | --- |
| `--all` | 显示所有状态 |
| `-t`, `--states` | 按状态过滤 |
| `-j`, `--jobs` | 按 job ID 过滤 |
| `-u`, `--user` | 按 user 过滤 |
| `-p`, `--partition` | 按 partition 过滤 |
| `-o`, `--format` | 选择输出字段 |
| `-S`, `--sort` | 指定排序方式 |
| `-l`, `--long` | 使用长格式默认视图 |
| `--start` | 显示预计开始时间 |
| `--array` | 显示数组风格的 job ID |
| `--noheader` | 省略表头 |

### 默认视图

```text
JOBID | PARTITION | NAME | USER | ST | TIME | NODELIST(REASON)
```

### 长视图

```text
JOBID | PARTITION | NAME | USER | ST | TIME | TIME_LIMIT | NTASKS | CPUS | REQ_MEM | REQ_GPU | NODELIST(REASON)
```

### 开始时间视图

当使用 `--start` 且未显式指定 format 时：

```text
JOBID | PARTITION | NAME | USER | ST | START_TIME | NODELIST(REASON)
```

### Format Fields

支持的字段名：

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

支持的 `%` 代码：

- `%i`
- `%P`
- `%j`
- `%u`
- `%t`, `%T`
- `%M`
- `%S`
- `%R`, `%N`

## `sacct`

`sacct` 显示持久化的记账数据，包括已完成的作业和 step。

### 常用选项

| 选项 | 含义 |
| --- | --- |
| `-j`, `--jobs` | 按 job ID 过滤 |
| `-s`, `--state` | 按状态过滤 |
| `-S`, `--starttime` | 按开始时间过滤 |
| `-E`, `--endtime` | 按结束时间过滤 |
| `-u`, `--user` | 按 user 过滤 |
| `-p`, `--partition` | 按 partition 过滤 |
| `-o`, `--format` | 选择输出字段 |
| `-P`, `--parsable2` | 使用 `|` 分隔输出 |
| `-n`, `--noheader` | 省略表头 |

### 默认视图

```text
JobID | Partition | JobName | User | State | ExitCode
```

### 记录类型

`sacct` 包含：

- 顶层 job
- allocation 记录
- step 记录
- 已完成记录

ID 展示规则：

- step ID 显示为 `<job_id>.<step_id>`
- array task 显示为 `<array_job_id>_<task_id>`

### Format Fields

支持的字段名：

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

支持的 `%` 代码：

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
