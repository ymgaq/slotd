# `srun` による対話実行

## 形式

```bash
srun [options] -- <command...>
```

## `srun` の動作

`srun` はデフォルトでコマンドを foreground で実行します。

挙動は、すでに allocation の中にいるかどうかで変わります。

- allocation の内側:
  - step record を作成する
  - foreground で直接コマンドを実行する
- allocation の外側:
  - allocation に近い top-level record を作成する
  - 実行可能になるまで待つ
  - step record を作成する
  - foreground でコマンドを実行する

daemon-managed な run job を投入するのは `--no-wait` の場合だけです。

## 主なオプション

| オプション | 意味 |
| --- | --- |
| `-J`, `--job-name <name>` | job name を設定する |
| `-p`, `--partition <partition>` | partition を選ぶ |
| `-c`, `--cpus-per-task <n>` | task ごとの CPU 数 |
| `-n`, `--ntasks <n>` | task 数 |
| `--mem <size>` | 要求メモリ |
| `-t`, `--time <time>` | time limit |
| `-G`, `--gpus <n>` | 要求 GPU slot 数 |
| `-o`, `--output <path>` | foreground stdout の出力先 |
| `-e`, `--error <path>` | foreground stderr の出力先 |
| `-D`, `--chdir <path>` | working directory |
| `--immediate` | すぐにリソースが確保できない場合は失敗する |
| `--pty` | foreground execution path を選ぶ |
| `--constraint <feature>` | 一致する local feature を要求する |
| `--cpu-bind <mode>` | CPU affinity を設定する |
| `--label` | 転送出力の先頭に `0: ` を付ける |
| `--unbuffered` | 転送出力を eager に flush する |
| `--no-wait` | daemon-managed run job として投入する |

## 出力挙動

例:

```bash
srun --label --unbuffered -- echo hello
```

典型的な出力:

```text
0: hello
```

## CPU Binding

サポートする値:

- `none`
- `cores`
- `map_cpu:<id,id,...>`

例:

```bash
srun --cpu-bind map_cpu:0,2 -- python train.py
```

## Immediate Mode

`--immediate` を付けると、リソースがすぐに確保できない場合に待たずに失敗します。

例:

```bash
srun --immediate -p gpu -G 1 -- nvidia-smi
```

## `--no-wait`

`--no-wait` は foreground で待たず、daemon に run job を投入します。

典型的な出力:

```text
Submitted run job 12
```

制限:

- `--label` と `--unbuffered` は `--no-wait` と同時には使えない
