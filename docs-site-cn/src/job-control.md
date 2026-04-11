# 作业控制

## `scontrol`

支持的形式：

```bash
scontrol show job <job_id>
scontrol hold job <job_id>
scontrol release job <job_id>
scontrol update job <job_id> KEY=VALUE...
```

### `show job`

显示详细作业信息，包括：

- 作业标识与所有者
- 状态与 reason
- 请求的资源
- 时间限制
- dependency 字符串
- submit、start、end 时间戳
- command 与 working directory
- stdout 与 stderr 路径
- array 元数据
- `ReqTRES`
- `AllocTRES`
- `MaxRSS`
- step 摘要

### `hold job`

把 pending job 移动到 held 状态。

结果：

- job 仍保持 `PENDING`
- reason 变为 `JobHeldUser`

### `release job`

将 held 的 pending job 释放回正常调度。

### `update job`

支持的更新键：

| 键 | 规则 |
| --- | --- |
| `JobName` / `Name` | 仅能在 job 为 `PENDING` 时修改 |
| `Partition` | 仅能在 job 为 `PENDING` 时修改 |
| `TimeLimit` / `Time` | 只要 job 尚未进入终态就可以修改 |
| `Priority` | 仅能在 job 为 `PENDING` 时修改 |

示例：

```bash
scontrol update job 10 TimeLimit=02:00:00
```

## `scancel`

支持的形式：

```bash
scancel <job_id>
scancel <job_id.step_id>
scancel --signal <sig> <job_id>
scancel --signal <sig> <job_id.step_id>
```

### 默认取消行为

- pending job 会立即变为 `CANCELLED`
- running job 会先进入 `COMPLETING`
- runner 会发送 `SIGTERM`
- 如果需要，宽限期后再发送 `SIGKILL`

记录的取消原因是：

```text
CancelledByUser
```

### Signal 模式

`--signal` 只发送指定信号，而不是执行默认取消流程。

示例：

```bash
scancel --signal TERM 12
```
