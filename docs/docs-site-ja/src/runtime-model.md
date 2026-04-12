# ランタイムモデル

## 全体像

`slotd` は単一ホスト向けのスケジューラです。実行モデルは意図的に単純です。

- 1 つのローカル daemon
- 1 つのローカル SQLite データベース
- 1 台のローカル実行ホスト
- 1 人のローカルユーザー向けワークフロー

controller と worker の分離はなく、リモートノード起動プロトコルもありません。

## 基本リソース

`slotd` が扱うリソースは 3 種類です。

- CPU
- メモリ
- GPU

現在の挙動:

- CPU 予約量は `ntasks * cpus-per-task`
- `ntasks` は batch と foreground 実行で task rank ごとに 1 つのローカルプロセスを起動する
- 総メモリの既定値は `/proc/meminfo` の `MemTotal` から検出し、失敗時は `16384 MB` にフォールバックする
- メモリは MB 単位で保存
- GPU は整数スロット
- admission は実使用量ではなく予約量ベース
- `SLOTD_CGROUP_BASE` が未設定なら、CPU とメモリは予約量だけを扱う
- `SLOTD_CGROUP_BASE` を writable な cgroup v2 subtree に設定すると、`slotd` は `memory.max` と `cpu.max` を書き込む
- 明示設定後に cgroup 設定に失敗した場合は、enforcement を黙って省略せず launch を失敗させる

## パーティション

環境変数で設定します。

- `SLOTD_CPU_PARTITIONS`
- `SLOTD_GPU_PARTITIONS`

ルール:

- 設定済みのパーティション名だけを受け付ける
- GPU がない場合、GPU パーティションは公開されない
- GPU パーティションを選び、かつ `--gpus` を省略した場合の既定値は `1`
- それ以外の既定値は `0`

## GPU 検出

`SLOTD_GPU_COUNT` が未設定の場合、`slotd` は `nvidia-smi` から GPU を検出しようとします。

現在の実装で確認するパス:

- `nvidia-smi`
- `/usr/bin/nvidia-smi`
- `/usr/lib/wsl/lib/nvidia-smi`
- `/bin/nvidia-smi`

## ジョブ種別

永続化される record は次のいずれかです。

- top-level batch job
- allocation-only job
- array task
- allocation 配下の step

## ジョブ状態

実装されている状態:

- `PENDING`
- `RUNNING`
- `COMPLETING`
- `COMPLETED`
- `FAILED`
- `CANCELLED`
- `TIMEOUT`
- `OUT_OF_MEMORY`

終端状態:

- `COMPLETED`
- `FAILED`
- `CANCELLED`
- `TIMEOUT`
- `OUT_OF_MEMORY`

## スケジューリングルール

daemon loop は `300ms` ごとに回ります。

pending job がブロックされる条件:

- dependency
- array concurrency limit
- delayed start time
- exclusive host use
- 予約リソース不足
- user hold state

順序付け:

- 基本ルールは投入順
- 明示的な job priority で投入順を上書きできる
- array task は array group ごとに交互に実行される

## Runtime Files

`SLOTD_ROOT` 配下:

- `run/slotd.sock`: daemon socket
- `lib/state.db`: SQLite state
- `lib/jobs/<job_id>/script.sh`: batch script
- `lib/jobs/<job_id>/runner.sh`: daemon wrapper
- `lib/jobs/<job_id>/exit_status`: wrapper exit status

## 通知

`SLOTD_NOTIFY_CMD` が設定されている場合、`slotd` は終端状態になった top-level job に対してそのコマンドを実行します。

export される変数:

- `SLOTD_JOB_ID`
- `SLOTD_JOB_NAME`
- `SLOTD_JOB_STATE`
- `SLOTD_JOB_PARTITION`
- `SLOTD_JOB_REASON`
