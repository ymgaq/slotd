# ジョブ制御

## `scontrol`

サポートする形式:

```bash
scontrol show job <job_id>
scontrol hold job <job_id>
scontrol release job <job_id>
scontrol update job <job_id> KEY=VALUE...
```

### `show job`

次のような詳細 job 情報を表示します。

- job ID と所有者
- 状態と reason
- 要求リソース
- time limit
- dependency string
- submit、start、end timestamp
- command と working directory
- stdout と stderr の path
- array metadata
- `ReqTRES`
- `AllocTRES`
- `MaxRSS`
- step summary

### `hold job`

pending job を held state に移します。

結果:

- job state は `PENDING` のまま
- reason は `JobHeldUser` になる

### `release job`

held 状態の pending job を通常スケジューリングに戻します。

### `update job`

サポートする更新キー:

| キー | ルール |
| --- | --- |
| `JobName` / `Name` | job が `PENDING` の間だけ変更できる |
| `Partition` | job が `PENDING` の間だけ変更できる |
| `TimeLimit` / `Time` | job が終端状態になるまでは変更できる |
| `Priority` | job が `PENDING` の間だけ変更できる |

例:

```bash
scontrol update job 10 TimeLimit=02:00:00
```

## `scancel`

サポートする形式:

```bash
scancel <job_id>
scancel <job_id.step_id>
scancel --signal <sig> <job_id>
scancel --signal <sig> <job_id.step_id>
```

### Default Cancel Behavior

- pending job はただちに `CANCELLED` になる
- running job は `COMPLETING` を経由する
- runner は `SIGTERM` を送る
- 必要なら grace period 後に `SIGKILL` を送る

記録される cancel reason:

```text
CancelledByUser
```

### Signal Mode

`--signal` は通常の cancel を行わず、指定シグナルだけを送ります。

例:

```bash
scancel --signal TERM 12
```
