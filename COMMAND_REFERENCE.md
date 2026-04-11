# slotd コマンドリファレンス

## 目的

この文書は、現在の `slotd` 実装が提供しているコマンド体系と実行時挙動をまとめた実装リファレンスです。ロードマップではなく、現時点で実際に使える機能を説明します。

関連文書:

- [DESIGN.md](/home/yu_yamaguchi/workspace/slotd/DESIGN.md): 設計方針
- [IMPLEMENTED.md](/home/yu_yamaguchi/workspace/slotd/IMPLEMENTED.md): 実装済み機能の要約

## スコープ

`slotd` は単一ホスト上で動作する Slurm 風スケジューラです。

- ローカル daemon 1 つ
- ローカル SQLite DB 1 つ
- 実行ノードは 1 台のみ
- batch job、interactive run、allocation、step を扱う
- CPU、memory、GPU は予約ベースで管理する
- `SLOTD_CGROUP_BASE` を設定した場合のみ cgroup v2 による制限を試みる

multi-node Slurm controller ではありません。

## バイナリとエイリアス

メインバイナリは `slotd` です。

利用可能なサブコマンド:

- `slotd daemon`
- `slotd sbatch`
- `slotd srun`
- `slotd salloc`
- `slotd squeue`
- `slotd sacct`
- `slotd scontrol`
- `slotd scancel`
- `slotd sinfo`

`argv[0]` によるエイリアス起動も実装されています。

- `sbatch`
- `srun`
- `salloc`
- `squeue`
- `sacct`
- `scontrol`
- `scancel`
- `sinfo`

## ランタイム配置

デフォルトのランタイムルートは `var/` です。

主なパス:

- socket: `var/run/slotd.sock`
- database: `var/lib/state.db`
- jobs: `var/lib/jobs/<job_id>/`
- batch script: `var/lib/jobs/<job_id>/script.sh`
- daemon wrapper script: `var/lib/jobs/<job_id>/runner.sh`
- daemon exit status file: `var/lib/jobs/<job_id>/exit_status`

ランタイムルートは `SLOTD_ROOT` で変更できます。`SLOTD_ROOT` 未指定時は相対パスの `var/` を使うため、daemon と client は同じ作業ディレクトリ、または同じ絶対パスの `SLOTD_ROOT` を共有する必要があります。

## 設定用環境変数

### パーティション

- `SLOTD_CPU_PARTITIONS`
  - CPU partition 名のカンマ区切り
  - デフォルトは `cpu`
- `SLOTD_GPU_PARTITIONS`
  - GPU partition 名のカンマ区切り
  - GPU が存在する場合のデフォルトは `gpu`

### リソース

- `SLOTD_GPU_COUNT`
  - ローカルホストの GPU スロット数
  - 未設定時は `nvidia-smi` から推定を試みる
- `SLOTD_GPU_MODEL`
  - GPU 表示名
  - 未設定時は `nvidia-smi` から推定を試みる
- `SLOTD_FEATURES`
  - `--constraint` 評価に使う feature 名のカンマ区切り
  - 暗黙に `cpu` が追加され、GPU がある場合は `gpu` も追加される

### 実行制御

- `SLOTD_CGROUP_BASE`
  - cgroup v2 のベースディレクトリ
  - 設定時は job ごとの cgroup を作成して `memory.max` と `cpu.max` を設定する
- `SLOTD_NOTIFY_CMD`
  - terminal な top-level job 完了時に `/bin/sh -lc` で実行する通知 hook

### `sbatch` の環境変数オーバーライド

`sbatch` では、対応する `SBATCH_*` 環境変数が `#SBATCH` より優先されます。主な対応項目:

- `SBATCH_JOB_NAME`
- `SBATCH_PARTITION`
- `SBATCH_CPUS_PER_TASK`
- `SBATCH_NTASKS`
- `SBATCH_MEM`
- `SBATCH_TIME`
- `SBATCH_GPUS`
- `SBATCH_CONSTRAINT`
- `SBATCH_BEGIN`
- `SBATCH_EXCLUSIVE`
- `SBATCH_REQUEUE`
- `SBATCH_OUTPUT`
- `SBATCH_ERROR`
- `SBATCH_CHDIR`
- `SBATCH_DEPENDENCY`
- `SBATCH_ARRAY_INX`
- `SBATCH_EXPORT`
- `SBATCH_EXPORT_FILE`
- `SBATCH_OPEN_MODE`
- `SBATCH_SIGNAL`

`SBATCH_EXCLUSIVE` と `SBATCH_REQUEUE` は `1|true|yes` を真として扱います。

## リソースモデル

`slotd` が予約・スケジューリング対象として扱うのは次です。

- CPU
- memory
- GPU

現在の挙動:

- CPU 予約量は `ntasks * cpus-per-task`
- memory は MB 単位で保持する
- GPU は整数スロット数で保持する
- admission は予約量ベースで、実時間の使用率では判定しない
- 実行ノードは常にローカルホスト 1 台のみ

デフォルト値:

- CPUs: `available_parallelism()`
- memory: `16384 MB`
- GPUs: `SLOTD_GPU_COUNT` または `nvidia-smi` の結果

partition の挙動:

- 設定済み partition 名のみ受け付ける
- GPU が無い場合は GPU partition を出さない
- GPU partition で `--gpus` 未指定時の既定値は `1`
- それ以外の partition で `--gpus` 未指定時の既定値は `0`
- GPU job 開始時は空いている GPU ID を割り当て、`CUDA_VISIBLE_DEVICES` に設定する

## ジョブモデル

永続化されるレコードは次のいずれかです。

- 通常の batch job
- allocation-only job
- array task job
- allocation 配下の step job

主要フィールド:

- job id
- parent job id
- step id
- job name
- user name
- partition
- command
- working directory
- requested CPUs / tasks / memory / GPUs
- dependency
- array metadata
- `requeue` と `requeue_count`
- time limit
- pid / pgid
- exit code / terminating signal / state reason
- `max_rss_kb`

## ジョブ状態

実装済み状態:

- `PENDING`
- `RUNNING`
- `COMPLETING`
- `COMPLETED`
- `FAILED`
- `CANCELLED`
- `TIMEOUT`
- `OUT_OF_MEMORY`

短縮表記:

- `PD`
- `R`
- `CG`
- `CD`
- `F`
- `CA`
- `TO`
- `OOM`

terminal state:

- `COMPLETED`
- `FAILED`
- `CANCELLED`
- `TIMEOUT`
- `OUT_OF_MEMORY`

## スケジューリング規則

daemon loop は `300ms` 間隔で動作します。

`PENDING` job の admission 時に見るもの:

- held job は `JobHeldUser` reason のまま待機
- dependency 未解決なら `Dependency`
- array の `%limit` に引っかかると `JobArrayTaskLimit`
- リソース不足なら `Resources`
- `begin_time` に達していない job は待機
- `exclusive` job は他の top-level running job と共存しない

並び順:

- 基本は submission order
- `Priority` が設定されている場合は pending queue で優先される
- array task は array group 単位で交互に取り出され、同一 array が連続占有しにくい

full Slurm の fairshare / QoS / preemption / backfill はありません。

## `#SBATCH` 解析

`#SBATCH` は script 先頭のコメントブロックのみ解析します。

挙動:

- `#!` 行は無視する
- 空行とコメント行は許可する
- 最初の非空・非コメント・非 `#SBATCH` 行に到達した後の `#SBATCH` は無視する
- 優先順位は `CLI > SBATCH_* 環境変数 > #SBATCH > built-in defaults`

対応 `#SBATCH`:

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

## 出力パス展開

batch output pattern では次を使えます。

- `%j`: job id
- `%A`: array job id
- `%a`: array task id
- `%x`: job name
- `%u`: user name
- `%N`: hostname
- `%%`: `%`

既定値:

- 非 array job の stdout: `slurm-%j.out`
- array job の stdout: `slurm-%A_%a.out`
- `--error` 未指定時の stderr は stdout と同じファイル

相対パスは job の working directory 基準で解決されます。

## export される環境変数

foreground 実行では子プロセスに次を設定します。

- `SLURM_JOB_ID`
- `SLURM_JOB_NAME`
- `SLURM_JOB_PARTITION`
- `SLURM_JOB_NODELIST`
- `SLURM_SUBMIT_DIR`
- `SLURM_NTASKS`
- `SLURM_CPUS_PER_TASK`
- `SLURM_STEP_ID`

array job では追加で:

- `SLURM_ARRAY_JOB_ID`
- `SLURM_ARRAY_TASK_ID`

GPU job では追加で:

- `CUDA_VISIBLE_DEVICES`

allocation 内 step では:

- `SLURM_JOB_ID` は親 allocation id
- `SLURM_STEP_ID` は step id

## コマンド別仕様

### `slotd daemon`

ローカル scheduler daemon を起動します。

挙動:

- runtime directory を作成する
- 既存 socket file があれば削除する
- UNIX socket を bind する
- SQLite state を開く
- restart recovery を試みる
- running job の poll、timeout 処理、pending job の scheduling を繰り返す

daemon は foreground に居続けます。

### `slotd sbatch`

batch job または `--wrap` の command を投入します。

形式:

```bash
slotd sbatch [options] <script>
slotd sbatch [options] --wrap '<command>'
```

対応オプション:

- `--wrap <command>`
- `-J`, `--job-name <name>`
- `-p`, `--partition <partition>`
- `-c`, `--cpus-per-task <n>`
- `-n`, `--ntasks <n>`
- `--mem <size>`
- `-t`, `--time <time>`
- `-G`, `--gpus <n>`
- `-o`, `--output <path>`
- `-e`, `--error <path>`
- `-D`, `--chdir <path>`
- `--constraint <feature>`
- `-d`, `--dependency <spec>`
- `-a`, `--array <spec>`
- `--export <spec>`
- `--export-file <path>`
- `--open-mode <append|truncate>`
- `--signal <signal[@seconds]>`
- `--begin <time>`
- `--exclusive`
- `--requeue`
- `--parsable`
- `-W`, `--wait`

挙動:

- script mode はファイル内容を読み込み、job directory に `script.sh` として保存する
- `--wrap` は内部的に shell script を生成する
- job は `PENDING` で永続化される
- 既定 partition は設定済み default partition
- 既定 resource は `cpus=1`, `ntasks=1`, `mem=512MB`
- GPU 既定値は partition に依存する
- `--begin` は epoch 秒、`YYYY-MM-DD`、`YYYY-MM-DDTHH:MM:SS`、`now+<duration>` を受け付ける
- `--exclusive` を付けた top-level job は単一ホストを排他的に使う
- `--parsable` は job id のみ出力する
- 通常は `Submitted batch job <id>` を出力する
- `--wait` は完了を待ち、失敗時は非 0 で返す
- array job の `--wait` は array root 配下の全 task 完了を待つ

dependency 形式:

- `after:<jobid>[,<jobid>...]`
- `afterany:<jobid>[,<jobid>...]`
- `afterok:<jobid>[,<jobid>...]`
- `afternotok:<jobid>[,<jobid>...]`
- `singleton`

array 形式:

- 単一 task の列挙
- range: `0-7`
- stepped range: `0-15:2`
- concurrency limit: `0-31%4`

`--export`:

- `ALL` で現在環境を引き継ぐ
- `NONE` で seed を消す
- `KEY=VALUE,...` を追加できる

`--signal`:

- signal 名または番号を解釈する
- `B:` prefix は受け付けるが、現状では batch script 単位の warning signal として扱う
- offset 未指定時は `60` 秒前

`--requeue`:

- top-level job が `FAILED`、`TIMEOUT`、`OUT_OF_MEMORY` で終わった場合に 1 回だけ `PENDING` へ戻す
- `COMPLETED` と `CANCELLED` は auto-requeue しない

注意:

- full Slurm のような array umbrella parent record は持たない
- array task は通常 job record として永続化され、最初の task が array root id になる

### `slotd srun`

foreground で command を実行します。`--no-wait` の場合のみ daemon-managed job を submit します。

形式:

```bash
slotd srun [options] -- <command...>
```

対応オプション:

- `-J`, `--job-name <name>`
- `-p`, `--partition <partition>`
- `-c`, `--cpus-per-task <n>`
- `-n`, `--ntasks <n>`
- `--mem <size>`
- `-t`, `--time <time>`
- `-G`, `--gpus <n>`
- `-o`, `--output <path>`
- `-e`, `--error <path>`
- `-D`, `--chdir <path>`
- `--immediate`
- `--pty`
- `--constraint <feature>`
- `--cpu-bind <mode>`
- `--label`
- `--unbuffered`
- `--no-wait`

実行モード:

- 既存 allocation 内:
  - step record を作り、foreground で直接実行する
- allocation 外:
  - allocation-like top-level record を作る
  - `RUNNING` になるまで待つ
  - foreground で実行する
  - accounting 用 step record も作る
- `--no-wait`:
  - daemon-managed command job を submit する
  - `Submitted run job <id>` を出す

挙動:

- 既定 resource は `sbatch` と同じ
- 既定 job name は command basename
- `--immediate` は即時に資源が取れなければ失敗する
- `--constraint` はローカル host の feature に対して評価する
- `--pty` は foreground path を選ぶだけで、完全な terminal allocation 機能ではない
- `--cpu-bind` は `none`、`cores`、`map_cpu:<id,id,...>` をサポートする
- `--label` は foreground 出力の各行頭に `0: ` を付ける
- `--unbuffered` は foreground 転送時に即 flush する
- foreground 実行の終了コードは caller に返す

### `slotd salloc`

allocation を取り、その中で foreground command を実行します。

形式:

```bash
slotd salloc [options] [command...]
```

対応オプション:

- `-J`, `--job-name <name>`
- `-p`, `--partition <partition>`
- `-c`, `--cpus-per-task <n>`
- `-n`, `--ntasks <n>`
- `--mem <size>`
- `-t`, `--time <time>`
- `-G`, `--gpus <n>`
- `-D`, `--chdir <path>`
- `--constraint <feature>`
- `--immediate`

挙動:

- command 未指定時は shell を起動する
- top-level record は `allocation_only = true` として作られる
- `Granted job allocation <id>` を出力する
- allocation が `RUNNING` になるまで待つ
- その後、Slurm 風環境変数付きで foreground 実行する
- 終了コードは caller に返る

### `slotd squeue`

永続化された top-level job の queue 状態を表示します。

対応オプション:

- `--all`
- `-t`, `--states <state1,state2,...>`
- `-j`, `--jobs <id1,id2,...>`
- `-u`, `--user <name>`
- `-p`, `--partition <name1,name2,...>`
- `-o`, `--format <spec>`
- `-S`, `--sort <spec>`
- `-l`, `--long`
- `--start`
- `--array`
- `--noheader`

現在の挙動:

- 既定 filter は `PENDING,RUNNING`
- `--all` で既定 state filter を外す
- step は表示しない
- `--array` は array task の JOBID を `<array_job_id>_<task_id>` 形式で出す
- `--start` を付けると推定開始時刻を `START_TIME` 列として表示する

既定列:

- `JOBID`
- `PARTITION`
- `NAME`
- `USER`
- `ST`
- `TIME`
- `NODELIST(REASON)`

`-l/--long` の既定列:

- `JOBID`
- `PARTITION`
- `NAME`
- `USER`
- `ST`
- `TIME`
- `TIME_LIMIT`
- `NTASKS`
- `CPUS`
- `REQ_MEM`
- `REQ_GPU`
- `NODELIST(REASON)`

`--start` を `-o` なしで使う場合の既定列:

- `JOBID`
- `PARTITION`
- `NAME`
- `USER`
- `ST`
- `START_TIME`
- `NODELIST(REASON)`

`-o/--format` で使える field 名:

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

`%` 形式で使える code:

- `%i`
- `%P`
- `%j`
- `%u`
- `%t`, `%T`
- `%M`
- `%S`
- `%R`, `%N`

sort key:

- `i`, `jobid`
- `p`, `partition`
- `u`, `user`
- `t`, `state`
- `m`, `time`

先頭に `-` を付けると降順です。

### `slotd sacct`

完了済み job や step を含む accounting 情報を表示します。

対応オプション:

- `-j`, `--jobs <id1,id2,...>`
- `-s`, `--state <state1,state2,...>`
- `-S`, `--starttime <timestamp>`
- `-E`, `--endtime <timestamp>`
- `-u`, `--user <name>`
- `-p`, `--partition <name1,name2,...>`
- `-o`, `--format <spec>`
- `-P`, `--parsable2`
- `-n`, `--noheader`

現在の挙動:

- top-level job と step の両方を表示する
- step id は `<job_id>.<step_id>`
- array task は `<array_job_id>_<task_id>`
- `ExitCode` は `<exit_code>:<signal>`
- `-P/--parsable2` は `|` 区切り

受け付ける時刻形式:

- `YYYY-MM-DD`
- `YYYY-MM-DDTHH:MM:SS`

既定列:

- `JobID`
- `Partition`
- `JobName`
- `User`
- `State`
- `ExitCode`

`-o/--format` で使える field 名:

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

`%` 形式で使える code:

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

### `slotd scontrol`

対応形式:

```bash
slotd scontrol show job <job_id>
slotd scontrol hold job <job_id>
slotd scontrol release job <job_id>
slotd scontrol update job <job_id> KEY=VALUE...
```

対応 update key:

- `JobName`, `Name`
- `Partition`
- `TimeLimit`, `Time`
- `Priority`

変更可能条件:

- `JobName`: `PENDING` 中のみ
- `Partition`: `PENDING` 中のみ
- `TimeLimit`: terminal state 前まで
- `Priority`: `PENDING` 中のみ

`show job` の出力内容:

- identity / ownership
- state / reason
- requested resources
- time limit
- dependency
- submit / start / end timestamp
- exit code
- array metadata
- batch flag
- working directory
- command
- stdout / stderr path
- nodelist
- `ReqTRES`
- `AllocTRES`
- `MaxRSS`
- step summary

### `slotd scancel`

top-level job または記録済み step を cancel または signal します。

形式:

```bash
slotd scancel <job_id>
slotd scancel <job_id.step_id>
slotd scancel --signal <sig> <job_id>
slotd scancel --signal <sig> <job_id.step_id>
```

挙動:

- `PENDING` job は即座に `CANCELLED`
- `RUNNING` job は一度 `COMPLETING` を経由する
- cancel 時は `SIGTERM` を送り、grace period 後に `SIGKILL`
- reason は `CancelledByUser`
- 終了時に cgroup OOM が検出されれば最終 state は `OUT_OF_MEMORY`
- `--signal` は running job に任意 signal を送る

### `slotd sinfo`

単一ノード上の partition 状態を表示します。

対応オプション:

- `-p`, `--partition <name1,name2,...>`
- `-N`, `--Node`
- `-l`, `--long`
- `-o`, `--format <spec>`
- `--noheader`

現在の挙動:

- 設定済み partition ごとに 1 行表示する
- default partition には `*` を付ける
- `-N` は受理するが、単一ノードなので表示粒度は partition 単位のまま
- `-l` は long format に切り替わる

既定列:

- `PARTITION`
- `HOSTNAMES`
- `STATE`
- `FEATURES`
- `GRES_USED`

`-l/--long` の既定列:

- `PARTITION`
- `HOSTNAMES`
- `STATE`
- `FEATURES`
- `CPUS`
- `CPU_ALLOC`
- `MEMORY`
- `MEM_ALLOC`
- `GPUS`
- `GPU_ALLOC`
- `RUNNING`
- `PENDING`
- `GRES_USED`

`-o/--format` で使える field 名:

- `Partition`
- `Hostnames`, `Hostname`, `NodeList`
- `State`
- `Features`
- `CPUS`
- `CPU_ALLOC`, `CPUSLOAD`, `CPUALLOC`
- `Memory`, `Mem`
- `MEM_ALLOC`, `MemoryAllocated`, `MemAlloc`
- `GPUS`
- `GPU_ALLOC`, `GpusAllocated`, `GpuAlloc`
- `Running`, `RunningJobs`
- `Pending`, `PendingJobs`
- `GRES_USED`, `GresUsed`

`%` 形式で使える code:

- `%P`
- `%N`
- `%t`, `%T`
- `%f`
- `%G`

## step と allocation の意味

`slotd` は次を区別します。

- top-level job
- allocation-only job
- allocation 配下の step

現在の挙動:

- `salloc` は allocation-only な top-level job を作る
- allocation 外の foreground `srun` は allocation-like top-level job と step を作る
- allocation 内 `srun` は step のみ作る
- `sacct` には step が出る
- `squeue` には step が出ない
- `scontrol show job <allocation_id>` には step summary が出る
- `scancel <job.step>` は step record を解決してその子レコードを対象にする

## runtime enforcement と OOM

`SLOTD_CGROUP_BASE` 設定時:

- daemon launch job で job ごとの cgroup を作る
- foreground allocation / step でも cgroup を作る
- `memory.max` に requested memory を書く
- `cpu.max` に requested CPU 相当を書き込む
- child pid を `cgroup.procs` に書く

OOM:

- cgroup memory events で OOM が見えたら `OUT_OF_MEMORY`
- それ以外の signal 終了は基本的に `FAILED`

cgroup 未設定時:

- scheduler による予約制御は動く
- runtime enforcement は best-effort

## recovery

daemon restart 後に running job の回復を試みます。

現在の挙動:

- daemon-managed job は wrapper script で `exit_status` を保存する
- recovery 時に process group の生存確認を行う
- process group が消えていれば `exit_status` から最終状態を復元する
- cgroup memory event があれば `OUT_OF_MEMORY` を優先する
- 十分な情報が無ければ `FAILED` / `LostAfterRestart` に落とす

## 通知

`SLOTD_NOTIFY_CMD` が設定されている場合、terminal な top-level job 完了時に best-effort で shell command を spawn します。

渡される環境変数:

- `SLOTD_JOB_ID`
- `SLOTD_JOB_NAME`
- `SLOTD_JOB_STATE`
- `SLOTD_JOB_PARTITION`
- `SLOTD_JOB_REASON`

requeue により `PENDING` へ戻った中間失敗では通知しません。

## Slurm 互換性の境界

Slurm 風に実装されているもの:

- command 名
- 主な submission flag
- `#SBATCH` 解析
- dependency
- array
- `salloc` と allocation 内 `srun`
- `scontrol show/hold/release/update job`
- `sacct`, `squeue`, `sinfo` の custom formatting

full Slurm と異なる主な点:

- single-node only
- multi-node placement はない
- account / QoS はない
- `scontrol` は `job` に限定
- `%` format の対応は部分集合
- array umbrella parent record はない
- `--pty` は完全な terminal 管理ではない
- `sstat` 風機能と `sattach` 風機能は未実装
