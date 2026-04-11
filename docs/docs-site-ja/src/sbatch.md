# `sbatch` によるバッチジョブ

## 形式

```bash
sbatch [options] <script>
sbatch [options] --wrap '<command>'
```

## `sbatch` の動作

`sbatch` は永続化された batch job record を作成し、ローカル daemon に投入します。

script mode の場合:

- ディスク上の script を読む
- job directory に本文を保存する
- 先頭の `#SBATCH` directive を解釈する

`--wrap` mode の場合:

- コマンドを包む内部 shell script を生成する

典型的な出力:

```text
Submitted batch job 1
```

`--parsable` を付けた場合:

```text
1
```

## 主なオプション

| オプション | 意味 |
| --- | --- |
| `--wrap <command>` | インラインの shell command を投入する |
| `-J`, `--job-name <name>` | job name を設定する |
| `-p`, `--partition <partition>` | partition を選ぶ |
| `-c`, `--cpus-per-task <n>` | task ごとの CPU 数 |
| `-n`, `--ntasks <n>` | task 数 |
| `--mem <size>` | 要求メモリ。`512M` や `8G` など |
| `-t`, `--time <time>` | time limit |
| `-G`, `--gpus <n>` | 要求 GPU slot 数 |
| `-o`, `--output <path>` | stdout の path pattern |
| `-e`, `--error <path>` | stderr の path pattern |
| `-D`, `--chdir <path>` | working directory |
| `--constraint <feature>` | 一致する local feature を要求する |
| `-d`, `--dependency <spec>` | dependency expression |
| `-a`, `--array <spec>` | array specification |
| `--export <spec>` | job 内に export する環境変数を指定する |
| `--export-file <path>` | ファイルから環境変数を読み込む |
| `--open-mode append\|truncate` | 出力ファイルに追記するか上書きするかを選ぶ |
| `--signal <spec>` | timeout 前に warning signal を送る |
| `--begin <time>` | job の実行可能時刻を遅らせる |
| `--exclusive` | 他の top-level job と host を共有しない |
| `--requeue` | 一部の失敗状態の後に 1 回だけ再投入する |
| `--parsable` | job ID だけを出力する |
| `-W`, `--wait` | job 完了まで待つ |

## 既定値

指定しない場合:

- `cpus-per-task = 1`
- `ntasks = 1`
- `mem = 512M`
- partition = 設定済みの default partition
- GPU の既定値は、GPU partition では `1`、それ以外では `0`

## `#SBATCH` 対応

サポートする directive:

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

優先順位:

1. command-line option
2. `SBATCH_*` 環境変数
3. `#SBATCH` directive
4. built-in default

## Dependencies

サポートする dependency expression:

- `after:<jobid>[,<jobid>...]`
- `afterany:<jobid>[,<jobid>...]`
- `afterok:<jobid>[,<jobid>...]`
- `afternotok:<jobid>[,<jobid>...]`
- `singleton`

## Arrays

サポートする array 指定:

- 単一 ID
- `0-7` のような range
- `0-15:2` のような step 付き range
- `0-31%4` のような concurrency limit

例:

```bash
sbatch -a 0-9%2 --wrap 'echo task=$SLURM_ARRAY_TASK_ID'
```

期待される結果:

- 複数の task record が永続化される
- 同じ array の同時実行数は最大 2

## Delayed Start

`--begin` が受け付ける形式:

- epoch seconds
- `YYYY-MM-DD`
- `YYYY-MM-DDTHH:MM:SS`
- `now+<duration>`

例:

```bash
sbatch --begin now+00:10:00 --wrap 'echo delayed'
```

## Requeue Once

`--requeue` による失敗時の扱い:

- `FAILED` は 1 回だけ requeue
- `TIMEOUT` は 1 回だけ requeue
- `OUT_OF_MEMORY` は 1 回だけ requeue
- `COMPLETED` は requeue しない
- `CANCELLED` は requeue しない

例:

```bash
sbatch --requeue --wrap 'exit 1'
```

## Output Paths

pattern token:

- `%j`: job ID
- `%A`: array job ID
- `%a`: array task ID
- `%x`: job name
- `%u`: user name
- `%N`: hostname
- `%%`: リテラルの `%`

既定値:

- 非 array の stdout: `slurm-%j.out`
- array の stdout: `slurm-%A_%a.out`
- `--error` 未指定時、stderr は stdout と同じ

## Environment Export

`--export` が受け付ける形式:

- `ALL`
- `NONE`
- `KEY=VALUE,...`

例:

```bash
sbatch --export FOO=bar,HELLO=world --wrap 'echo "$FOO $HELLO"'
```

期待される結果:

- 出力に `bar world` が含まれる
