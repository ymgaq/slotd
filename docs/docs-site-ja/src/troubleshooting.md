# トラブルシューティング

## `Connection refused`

症状:

```text
error: io error: Connection refused (os error 111)
```

意味:

- client は socket path を見つけている
- ただしその場所で接続を受け付ける daemon が動いていない

確認ポイント:

- daemon が実際に起動しているか
- client と daemon が同じ `SLOTD_ROOT` を使っているか
- 古い実行で残った stale socket file がないか

確認コマンド:

```bash
ls -l "$SLOTD_ROOT/run/slotd.sock"
sinfo
```

## Runtime Root が違う

daemon と client が別の runtime root を使うと、コマンドは不安定に失敗したように見えます。

よくある原因:

- daemon は `systemd --user` で起動している
- client は同じ `SLOTD_ROOT` を持たない shell から起動している

最も簡単な対策:

- `scripts/install.sh` でインストールする
- `~/.local/bin` の wrapper command を使う

## GPU Partition が出ない

`sinfo` に `cpu` しか出ない場合の確認項目:

- daemon 環境で `nvidia-smi` が動くか
- `SLOTD_GPU_PARTITIONS` が意図通りに設定されているか
- GPU 設定変更後に daemon を再起動したか

手動確認:

```bash
nvidia-smi
sinfo
```

## 再インストール時の `Text file busy`

インストーラは現在、バイナリを atomic に置き換えます。それでも問題が出る場合:

- daemon を停止または再起動する
- もう一度インストーラを実行する

コマンド:

```bash
systemctl --user restart slotd.service
./scripts/install.sh
```

## Job が `PENDING` のまま

考えられる原因:

- リソース不足
- dependency 未達成
- array concurrency limit
- delayed start time 未到達
- job が held
- すでに exclusive job が動いている

確認方法:

```bash
squeue
scontrol show job <job_id>
```

## cgroup 設定に失敗する

`SLOTD_CGROUP_BASE` を設定しているのに writable な cgroup v2 subtree を指して
いない場合、job launch は明示的な cgroup error で失敗します。

確認項目:

- daemon 環境でその path が存在するか
- 通常の directory や file ではなく cgroup v2 subtree か
- daemon ユーザーが job ごとの subdirectory 作成と control file 書き込みをできるか

## Cancel したのに `OUT_OF_MEMORY` で終わった

終了処理中に cgroup の memory event が OOM を示した場合、最終状態は `OUT_OF_MEMORY` が優先されることがあります。

## 出力ファイルが見当たらない

確認ポイント:

- job の working directory
- `-o/--output` と `-e/--error` の path
- `%j` や `%A_%a` などの pattern expansion

確認コマンド:

```bash
scontrol show job <job_id>
```
