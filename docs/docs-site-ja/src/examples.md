# 実行例

## CPU バッチジョブ

```bash
sbatch \
  -J hello \
  -p cpu \
  -c 1 \
  --mem 512M \
  -t 00:05:00 \
  -o logs/%j.out \
  --wrap 'echo hello'
```

期待される結果:

- `Submitted batch job <id>`
- `logs/<id>.out` に `hello` が書かれる

## GPU バッチジョブ

```bash
sbatch \
  -J gpu-demo \
  -p gpu \
  -c 4 \
  --mem 8G \
  -G 1 \
  -t 01:00:00 \
  -o logs/%j.out \
  --wrap 'nvidia-smi'
```

期待される結果:

- job は GPU partition 上で動く
- 出力に `nvidia-smi` の情報が含まれる

## 対話 foreground 実行

```bash
srun --label --unbuffered -- echo hello
```

期待される結果:

```text
0: hello
```

## 対話 allocation

```bash
salloc -p gpu -c 4 --mem 8G -G 1 -t 00:30:00
```

期待される結果:

- `Granted job allocation <id>`
- allocation の中で shell が起動する

## Array Job

```bash
sbatch \
  -J array-demo \
  -a 0-9%2 \
  -o logs/%A_%a.out \
  --wrap 'echo task=$SLURM_ARRAY_TASK_ID'
```

期待される結果:

- 複数の task record が作られる
- `logs/<array_id>_0.out` のような log file ができる

## 1 回だけの Requeue

```bash
sbatch --requeue --wrap 'exit 1'
```

期待される結果:

- 最初の失敗で job は `PENDING` に戻る
- 2 回目の失敗で最終状態は `FAILED` になる

## 遅延開始

```bash
sbatch --begin now+00:10:00 --wrap 'echo delayed'
```

期待される結果:

- begin 時刻までは pending のまま
- `squeue --start` に将来の開始時刻が出る

## 明示的な環境変数 export

```bash
sbatch \
  --export FOO=bar,HELLO=world \
  --wrap 'echo "$FOO $HELLO"'
```

期待される結果:

- 出力に `bar world` が含まれる

## daemon の手動起動

```bash
SLOTD_ROOT="$HOME/.local/share/slotd" ./target/release/slotd daemon
```

別シェルでは次を実行します。

```bash
SLOTD_ROOT="$HOME/.local/share/slotd" ./target/release/slotd sbatch --wrap 'echo hello'
```
