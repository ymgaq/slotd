# クイックスタート

## 1. daemon の確認

スクリプトでインストールし、`--skip-systemd` を使っていなければ、daemon はすでに起動しているはずです。

まず基本コマンドを確認します。

```bash
sinfo
squeue
sacct
```

初回起動時の典型例:

- `sinfo` は設定済みパーティションごとに 1 行表示する
- `squeue` は空
- `sacct` は空

## 2. 単純なバッチジョブを投入する

```bash
sbatch --wrap 'echo hello from slotd'
```

典型的な出力:

```text
Submitted batch job 1
```

## 3. キューを確認する

```bash
squeue
```

ジョブ実行中の典型的な出力:

```text
JOBID | PARTITION | NAME | USER | ST | TIME | NODELIST(REASON)
1     | cpu       | wrap | ...  | R  | 0:00 | localhost
```

## 4. 完了したジョブを確認する

```bash
sacct
```

ジョブ完了後の典型的な出力:

```text
JobID | Partition | JobName | User | State     | ExitCode
1     | cpu       | wrap    | ...  | COMPLETED | 0:0
```

## 5. ジョブの詳細を表示する

```bash
scontrol show job 1
```

ここで確認できる内容:

- ジョブ ID と所有者
- ジョブ状態と reason
- 要求リソース
- 出力パス
- 作業ディレクトリ
- 各種 timestamp

## 6. 対話実行を試す

```bash
srun --label --unbuffered -- echo hello
```

典型的な出力:

```text
0: hello
```
