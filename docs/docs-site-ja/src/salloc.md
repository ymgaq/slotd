# `salloc` によるアロケーション

## 形式

```bash
salloc [options] [command...]
```

## `salloc` の動作

`salloc` は allocation-only の top-level job を作成し、実行可能になるまで待ってから、その allocation の中で foreground command を起動します。

command を指定しない場合は、自分の shell を起動します。

典型的な出力:

```text
Granted job allocation 4
```

## 主なオプション

| オプション | 意味 |
| --- | --- |
| `-J`, `--job-name <name>` | allocation 名を設定する |
| `-p`, `--partition <partition>` | partition を選ぶ |
| `-c`, `--cpus-per-task <n>` | task ごとの CPU 数 |
| `-n`, `--ntasks <n>` | task 数 |
| `--mem <size>` | 要求メモリ |
| `-t`, `--time <time>` | time limit |
| `-G`, `--gpus <n>` | 要求 GPU slot 数 |
| `-D`, `--chdir <path>` | working directory |
| `--constraint <feature>` | 一致する local feature を要求する |
| `--immediate` | allocation をすぐに開始できない場合は失敗する |

## 例

```bash
salloc -p gpu -c 4 --mem 8G -G 1 -t 00:30:00
```

期待される結果:

- allocation record が作成される
- allocation が実行可能になるまで command が待つ
- allocation の中で shell が起動する
- その後の `srun` はその allocation 配下の step になる
