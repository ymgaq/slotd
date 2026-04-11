# Slurmユーザー向けコマンド・オプション・`#SBATCH`ディレクティブ調査メモ

この文書は、Slurmを利用する一般ユーザーが日常的に使うコマンド、主要オプション、`sbatch`および`#SBATCH`ディレクティブの挙動を整理したものです。内容は主にSchedMD公式ドキュメントを元に要約しています。

参照元:

- [Slurm Manual Index](https://slurm.schedmd.com/man_index.html)
- [Quick Start User Guide](https://slurm.schedmd.com/quickstart.html)
- [`sbatch`](https://slurm.schedmd.com/sbatch.html)
- [`srun`](https://slurm.schedmd.com/srun.html)
- [`salloc`](https://slurm.schedmd.com/salloc.html)
- [`squeue`](https://slurm.schedmd.com/squeue.html)
- [`scancel`](https://slurm.schedmd.com/scancel.html)
- [`sacct`](https://slurm.schedmd.com/sacct.html)
- [`sinfo`](https://slurm.schedmd.com/sinfo.html)
- [Job State Codes](https://slurm.schedmd.com/job_state_codes.html)

## 1. まず把握すべきSlurmコマンド

Slurm利用者が主に触るコマンドは次です。

| コマンド | 役割 | 典型例 |
| --- | --- | --- |
| `sbatch` | バッチジョブをキューに投入する | `sbatch job.sh` |
| `srun` | 対話実行、または割り当て内でジョブステップを起動する | `srun -n 4 hostname` |
| `salloc` | 対話用に資源だけ先に確保する | `salloc -N 1 -n 4` |
| `squeue` | 待機中・実行中ジョブを見る | `squeue -u "$USER"` |
| `scancel` | ジョブをキャンセルする、またはシグナル送信する | `scancel 12345` |
| `sacct` | 終了済みを含む履歴・会計情報を見る | `sacct -j 12345` |
| `sinfo` | パーティション・ノード状態を見る | `sinfo` |
| `scontrol` | ジョブやノードの詳細参照、限定的な変更を行う | `scontrol show job 12345` |

実務上の流れは概ね次です。

1. `sbatch`または`salloc`/`srun`で実行を依頼する
2. `squeue`で待ち・実行状態を確認する
3. 必要なら`scancel`で停止する
4. 終了後は`sacct`で結果と終了理由を確認する
5. 資源状況は`sinfo`や`scontrol show job`で深掘りする

## 2. `sbatch`の位置づけ

`sbatch`はバッチスクリプトをSlurmコントローラへ登録するコマンドです。投入が成功するとジョブIDが返り、実際の実行は後続のスケジューリングに委ねられます。

基本形:

```bash
sbatch [options] script [args...]
```

標準入力から与えることもできます。

```bash
sbatch <<'EOF'
#!/bin/bash
hostname
EOF
```

1行コマンドだけなら`--wrap`も使えます。

```bash
sbatch --wrap="python train.py --epochs 10"
```

ポイント:

- `sbatch`自体は通常すぐ終了する
- ジョブは資源が空くまで`PENDING`になりうる
- ジョブスクリプト内では通常のシェルスクリプトとして処理が進む
- 実処理の起動に`srun`を使う構成が一般的

## 3. `sbatch`スクリプトの基本例

```bash
#!/bin/bash
#SBATCH --job-name=train
#SBATCH --partition=gpu
#SBATCH --gpus=1
#SBATCH --cpus-per-task=8
#SBATCH --mem=32G
#SBATCH --time=04:00:00
#SBATCH --output=logs/%j.out
#SBATCH --error=logs/%j.err

set -euo pipefail

echo "job id: $SLURM_JOB_ID"
srun python train.py
```

## 4. `#SBATCH`ディレクティブの解釈規則

`#SBATCH`は、スクリプト内に書く`sbatch`オプションです。CLIに書くのと同種の指定をファイル内へ埋め込めます。

重要な規則:

- `#SBATCH`で始まる行だけがディレクティブとして扱われる
- 解釈されるのはスクリプト冒頭の連続したコメント領域だけ
- 最初の「空行でもコメントでもない行」に到達した時点で、その後ろの`#SBATCH`は無視される
- `#SBATCH`行はシェルによって評価されない
- そのため、シェル変数展開やコマンド置換を期待してはいけない

無視される例:

```bash
#!/bin/bash
echo start
#SBATCH --time=10:00
```

この`#SBATCH --time=10:00`は有効になりません。

安全な書き方:

```bash
#!/bin/bash
#SBATCH --time=10:00
#SBATCH --mem=8G

echo start
```

## 5. オプションの優先順位

実運用上は次の順で優先されると理解しておくとよいです。

1. `sbatch`コマンドライン引数
2. `#SBATCH`ディレクティブ
3. `SBATCH_*`系環境変数
4. クラスタの既定設定

例:

```bash
sbatch --job-name=cli-name job.sh
```

```bash
#!/bin/bash
#SBATCH --job-name=script-name
```

この場合、最終的なジョブ名は`cli-name`です。

## 6. `sbatch`の主要オプション

### 6.1 ジョブ識別

| オプション | 意味 | 例 |
| --- | --- | --- |
| `-J`, `--job-name=<name>` | ジョブ名を指定 | `sbatch -J train job.sh` |
| `--comment=<string>` | コメント文字列を付与 | `sbatch --comment=exp42 job.sh` |
| `--wckey=<key>` | Workload Characterization Keyを指定 | `sbatch --wckey=teamA job.sh` |

補足:

- ジョブ名の既定値は通常スクリプト名
- 標準入力から読む場合の既定ジョブ名は`sbatch`

### 6.2 パーティション・アカウント・QoS

| オプション | 意味 | 例 |
| --- | --- | --- |
| `-p`, `--partition=<names>` | パーティション指定 | `sbatch -p gpu job.sh` |
| `-A`, `--account=<account>` | 課金・利用枠アカウント指定 | `sbatch -A proj123 job.sh` |
| `--qos=<qos>` | QoS指定 | `sbatch --qos=high job.sh` |
| `--reservation=<name>` | 予約済み資源を使う | `sbatch --reservation=maint job.sh` |

補足:

- `--partition`はカンマ区切り複数指定が可能
- 実際に使える`account`や`qos`はクラスタ側設定に依存する

### 6.3 CPU・タスク・ノード関連

| オプション | 意味 | 例 |
| --- | --- | --- |
| `-N`, `--nodes=<min[-max]>` | 必要ノード数 | `sbatch -N 2 job.sh` |
| `-n`, `--ntasks=<num>` | 総タスク数 | `sbatch -n 16 job.sh` |
| `-c`, `--cpus-per-task=<n>` | 1タスクあたりCPU数 | `sbatch -c 8 job.sh` |
| `--ntasks-per-node=<n>` | 1ノードあたりタスク数 | `sbatch --ntasks-per-node=4 job.sh` |
| `--ntasks-per-core=<n>` | 1コアあたりタスク数 | `sbatch --ntasks-per-core=1 job.sh` |
| `--threads-per-core=<n>` | コアあたりスレッド数 | `sbatch --threads-per-core=1 job.sh` |
| `--hint=<type>` | バインディングやSMTのヒント | `sbatch --hint=nomultithread job.sh` |
| `--exclusive` | ノード占有 | `sbatch --exclusive job.sh` |
| `--oversubscribe` | オーバーサブスクライブ許可 | `sbatch --oversubscribe job.sh` |

考え方:

- `--ntasks`はプロセス数に近い
- `--cpus-per-task`は各プロセスに必要なCPU数
- MPI系は`--ntasks`
- OpenMPやスレッド並列は`--cpus-per-task`
- 両方使うハイブリッド構成では両者を明示する

### 6.4 メモリ・GPU・GRES

| オプション | 意味 | 例 |
| --- | --- | --- |
| `--mem=<size>` | ノードあたりメモリ量 | `sbatch --mem=64G job.sh` |
| `--mem-per-cpu=<size>` | CPUあたりメモリ量 | `sbatch --mem-per-cpu=4G job.sh` |
| `--mem-per-gpu=<size>` | GPUあたりメモリ量 | `sbatch --mem-per-gpu=16G job.sh` |
| `-G`, `--gpus=[type:]<n>` | GPU数指定 | `sbatch --gpus=a100:2 job.sh` |
| `--gpus-per-node=[type:]<n>` | ノードあたりGPU数 | `sbatch --gpus-per-node=4 job.sh` |
| `--gpus-per-task=[type:]<n>` | タスクあたりGPU数 | `sbatch --gpus-per-task=1 job.sh` |
| `--gres=<name[:type]:count>` | 汎用資源指定 | `sbatch --gres=gpu:2 job.sh` |
| `--constraint=<features>` | ノード属性制約 | `sbatch --constraint=avx512 job.sh` |

補足:

- `--mem`, `--mem-per-cpu`, `--mem-per-gpu`は排他的に扱う
- メモリ単位は通常`K`, `M`, `G`, `T`
- 既定単位はMB
- GPU系オプションの可否と書式はクラスタ設定に依存する
- GRESはGPU以外のライセンスや特殊デバイスにも使われる

### 6.5 実行時間・開始時刻・期限

| オプション | 意味 | 例 |
| --- | --- | --- |
| `-t`, `--time=<time>` | 制限時間 | `sbatch -t 02:30:00 job.sh` |
| `--time-min=<time>` | 許容最小時間 | `sbatch --time=4:00:00 --time-min=2:00:00 job.sh` |
| `--begin=<time>` | 指定時刻まで開始しない | `sbatch --begin=now+1hour job.sh` |
| `--deadline=<time>` | この時刻までに開始できないなら不適格 | `sbatch --deadline=2026-04-12T12:00:00 job.sh` |

時間指定の代表例:

- `minutes`
- `minutes:seconds`
- `hours:minutes:seconds`
- `days-hours`
- `days-hours:minutes`
- `days-hours:minutes:seconds`

例:

```bash
sbatch -t 90 job.sh
sbatch -t 01:30:00 job.sh
sbatch -t 2-00:00:00 job.sh
sbatch --begin=tomorrow+08:00 job.sh
```

### 6.6 依存関係

| オプション | 意味 | 例 |
| --- | --- | --- |
| `-d`, `--dependency=<spec>` | 他ジョブとの依存関係 | `sbatch -d afterok:12345 job.sh` |
| `--kill-on-invalid-dep=<yes|no>` | 不正依存時の扱い | `sbatch --kill-on-invalid-dep=yes ...` |

代表的な依存種別:

- `after:<jobid>`: 対象ジョブが開始またはキャンセルされた後
- `afterok:<jobid>`: 正常終了後
- `afternotok:<jobid>`: 異常終了後
- `afterany:<jobid>`: 終了状態を問わず終了後
- `singleton`: 同名ジョブが同一ユーザーで同時実行されないようにする

例:

```bash
sbatch --dependency=afterok:12345 train.sh
sbatch --dependency=afterany:12345,12346 collect.sh
sbatch --dependency=singleton nightly.sh
```

### 6.7 配列ジョブ

| オプション | 意味 | 例 |
| --- | --- | --- |
| `-a`, `--array=<spec>` | 配列ジョブを作る | `sbatch -a 0-99%8 job.sh` |

書式:

- `0-9`
- `1,3,5`
- `0-15:4`
- `0-99%8`

意味:

- `%8`は同時実行数上限
- 各要素は個別ジョブのように管理される
- 親ジョブIDと配列インデックスは区別される

関連環境変数:

- `SLURM_ARRAY_JOB_ID`
- `SLURM_ARRAY_TASK_ID`
- `SLURM_ARRAY_TASK_COUNT`
- `SLURM_ARRAY_TASK_MIN`
- `SLURM_ARRAY_TASK_MAX`

例:

```bash
#!/bin/bash
#SBATCH --array=0-15%4
#SBATCH --output=logs/%A_%a.out

python run_case.py --index "$SLURM_ARRAY_TASK_ID"
```

### 6.8 標準出力・標準エラー・入力

| オプション | 意味 | 例 |
| --- | --- | --- |
| `-o`, `--output=<pattern>` | 標準出力先 | `sbatch -o logs/%j.out job.sh` |
| `-e`, `--error=<pattern>` | 標準エラー先 | `sbatch -e logs/%j.err job.sh` |
| `-i`, `--input=<path>` | 標準入力元 | `sbatch -i input.txt job.sh` |
| `--open-mode=append|truncate` | 既存ファイルへの追記/上書き | `sbatch --open-mode=append ...` |

既定動作:

- `--output`未指定時は通常`slurm-%j.out`
- `--error`未指定時は標準出力と同じファイルにまとまることがある
- 実際の既定動作はクラスタ設定やSlurm版に依存する場合がある

ファイル名パターンでよく使う置換:

| パターン | 意味 |
| --- | --- |
| `%j` | jobid |
| `%J` | `jobid.stepid` |
| `%A` | 配列ジョブの親jobid |
| `%a` | 配列インデックス |
| `%N` | 短いホスト名 |
| `%n` | ジョブ内ノード番号 |
| `%t` | タスク番号 |
| `%u` | ユーザー名 |
| `%x` | ジョブ名 |
| `%%` | `%`そのもの |

例:

```bash
sbatch -o logs/%x-%j.out -e logs/%x-%j.err job.sh
sbatch --array=0-9 -o logs/%A_%a.out job.sh
```

### 6.9 作業ディレクトリ・環境変数・エクスポート

| オプション | 意味 | 例 |
| --- | --- | --- |
| `-D`, `--chdir=<dir>` | 実行前に作業ディレクトリ変更 | `sbatch -D /work/proj job.sh` |
| `--export=<spec>` | 環境変数の引き継ぎ制御 | `sbatch --export=ALL,FOO=bar job.sh` |
| `--get-user-env` | ログイン環境を取得 | `sbatch --get-user-env job.sh` |
| `--export-file=<file>` | ファイルから環境変数を読む | `sbatch --export-file=env.txt ...` |

`--export`の代表例:

- `--export=ALL`
- `--export=NONE`
- `--export=ALL,OMP_NUM_THREADS=8`
- `--export=VAR1,VAR2`

考え方:

- `ALL`は送信元環境を基本的に引き継ぐ
- `NONE`はほぼ引き継がない
- 再現性を重視するなら`--export=NONE`や明示列挙を検討する

### 6.10 ノード選択・除外・配置制約

| オプション | 意味 | 例 |
| --- | --- | --- |
| `-w`, `--nodelist=<nodes>` | 使用ノード指定 | `sbatch -w node01 job.sh` |
| `-x`, `--exclude=<nodes>` | 除外ノード指定 | `sbatch -x node03,node04 job.sh` |
| `--constraint=<features>` | ノード機能制約 | `sbatch --constraint=gpu&ib job.sh` |
| `--prefer=<features>` | 希望制約 | `sbatch --prefer=a100 job.sh` |
| `--contiguous` | 連続ノード要求 | `sbatch --contiguous ...` |
| `--distribution=<spec>` | タスク分布方式 | `sbatch --distribution=block ...` |

注意:

- ノード名やfeature名はクラスタ依存
- `--constraint`は論理式を扱える構成がある

### 6.11 通知・再実行・シグナル

| オプション | 意味 | 例 |
| --- | --- | --- |
| `--mail-type=<types>` | 通知タイミング | `sbatch --mail-type=END,FAIL ...` |
| `--mail-user=<addr>` | 通知先メール | `sbatch --mail-user=user@example.com ...` |
| `--requeue` | 再実行可能にする | `sbatch --requeue job.sh` |
| `--no-requeue` | 再実行禁止 | `sbatch --no-requeue job.sh` |
| `--signal=<spec>` | 終了前シグナル通知 | `sbatch --signal=B:USR1@60 job.sh` |

代表的な`--mail-type`:

- `BEGIN`
- `END`
- `FAIL`
- `REQUEUE`
- `ALL`

`--signal`の利用目的:

- タイムリミット直前にチェックポイント保存する
- 長時間学習ジョブの安全終了処理を入れる

### 6.12 その他よく使う指定

| オプション | 意味 | 例 |
| --- | --- | --- |
| `--parsable` | パースしやすい形式でjobid出力 | `jobid=$(sbatch --parsable job.sh)` |
| `--wrap=<command>` | スクリプトなしで1行投入 | `sbatch --wrap="hostname"` |
| `-W`, `--wait` | ジョブ終了まで待つ | `sbatch --wait job.sh` |
| `--test-only` | 実投入せず検証中心で処理 | `sbatch --test-only job.sh` |
| `--hold` | 保留状態で投入 | `sbatch --hold job.sh` |
| `--profile=<types>` | プロファイリング設定 | `sbatch --profile=task ...` |
| `--licenses=<spec>` | ライセンス資源要求 | `sbatch --licenses=matlab:1 ...` |

## 7. `#SBATCH`でよく使うディレクティブ一覧

`#SBATCH`では、ほぼそのままCLIオプションを記述します。

例:

```bash
#SBATCH --job-name=train
#SBATCH -p gpu
#SBATCH --gpus=1
#SBATCH --mem=32G
#SBATCH -t 04:00:00
#SBATCH --array=0-15%4
#SBATCH --dependency=afterok:12345
#SBATCH -o logs/%j.out
#SBATCH -e logs/%j.err
#SBATCH --mail-type=END,FAIL
#SBATCH --mail-user=user@example.com
```

ユーザーがよく使うディレクティブ群:

- `--job-name`
- `--partition`
- `--account`
- `--qos`
- `--nodes`
- `--ntasks`
- `--cpus-per-task`
- `--mem`
- `--mem-per-cpu`
- `--gpus`
- `--gres`
- `--constraint`
- `--time`
- `--begin`
- `--deadline`
- `--dependency`
- `--array`
- `--output`
- `--error`
- `--chdir`
- `--export`
- `--mail-type`
- `--mail-user`
- `--exclusive`
- `--requeue`
- `--signal`

## 8. `srun`の役割と主要オプション

`srun`は二つの使い方があります。

1. その場でジョブを起動する
2. 既に確保済みのジョブ割り当て内でジョブステップを起動する

例:

```bash
srun -n 4 hostname
salloc -N 1 -n 4
srun ./app
```

よく使うオプションは`sbatch`とかなり共通です。

| オプション | 意味 |
| --- | --- |
| `-n`, `--ntasks` | タスク数 |
| `-c`, `--cpus-per-task` | タスクあたりCPU |
| `-N`, `--nodes` | ノード数 |
| `-p`, `--partition` | パーティション |
| `--mem`, `--gpus`, `--gres` | 資源要求 |
| `--pty` | 疑似端末を付けた対話実行 |
| `--input`, `--output`, `--error` | I/O制御 |
| `--cpu-bind`, `--mem-bind`, `--gpu-bind` | バインド制御 |
| `--mpi=<type>` | MPI起動方式 |
| `--exclusive`, `--overlap` | 資源共有制御 |

典型例:

```bash
srun --pty bash
srun -N 2 -n 16 --mpi=pmix ./mpi_app
srun --cpu-bind=cores ./omp_app
```

## 9. `salloc`の役割

`salloc`は、対話作業用に資源だけを先に確保するコマンドです。確保後、そのシェルや子プロセスで`srun`を使って作業します。

例:

```bash
salloc -N 1 -n 4 -t 01:00:00
srun --pty bash
```

用途:

- 対話的なデバッグ
- ノード上での短時間検証
- Jupyterやシェルベース作業の前段

主要オプションは`sbatch`/`srun`とほぼ同系統です。

## 10. `squeue`での状態確認

`squeue`は現在キューに存在するジョブを見るコマンドです。待機中・実行中の確認に向きます。

典型例:

```bash
squeue
squeue -u "$USER"
squeue --jobs 12345,12346
squeue --states=PENDING,RUNNING
squeue --format="%.18i %.9P %.20j %.8u %.2t %.10M %.6D %R"
```

よく使うオプション:

| オプション | 意味 |
| --- | --- |
| `-u`, `--user=<name>` | ユーザーで絞る |
| `-j`, `--jobs=<ids>` | jobidで絞る |
| `-p`, `--partition=<name>` | パーティションで絞る |
| `-t`, `--states=<states>` | 状態で絞る |
| `-o`, `--format=<fmt>` | 出力フォーマット指定 |
| `-l`, `--long` | 長い形式 |
| `-r`, `--array` | 配列ジョブ要素を展開表示 |
| `-h`, `--noheader` | ヘッダ非表示 |
| `-S`, `--sort=<spec>` | ソート指定 |

`NODELIST(REASON)`列は重要です。

- 実行中ならノード名が見える
- 待機中なら理由が出る
- 代表例: `Resources`, `Priority`, `Dependency`, `ReqNodeNotAvail`

## 11. `scancel`での停止・シグナル送信

`scancel`はジョブ停止やシグナル送信に使います。

例:

```bash
scancel 12345
scancel 12345_7
scancel --signal=TERM 12345
scancel --user="$USER" --state=PENDING
```

よく使うオプション:

| オプション | 意味 |
| --- | --- |
| `--signal=<sig>` | 任意シグナル送信 |
| `-u`, `--user=<name>` | ユーザーで絞る |
| `-p`, `--partition=<name>` | パーティションで絞る |
| `-t`, `--state=<states>` | 状態で絞る |
| `-n`, `--name=<jobname>` | ジョブ名で絞る |
| `-w`, `--nodelist=<nodes>` | ノードで絞る |
| `-f`, `--full` | バッチステップも含める |
| `-b`, `--batch` | バッチステップのみに送る |

注意:

- `jobid_arrayindex`形式で配列ジョブ要素個別指定が可能
- フィルタ条件は組み合わせて使える
- 誤爆防止のため、まず`squeue`で対象確認してから使うべき

## 12. `sacct`での履歴・終了結果確認

`sacct`は完了済みも含めた履歴確認向けです。失敗解析では必須です。

例:

```bash
sacct -j 12345
sacct -j 12345 --format=JobID,JobName,Partition,State,ExitCode,Elapsed,MaxRSS
sacct -s FAILED,TIMEOUT
sacct -S 2026-04-01 -E 2026-04-11
```

よく使うオプション:

| オプション | 意味 |
| --- | --- |
| `-j`, `--jobs=<ids>` | jobid指定 |
| `-s`, `--state=<states>` | 状態で絞る |
| `-S`, `--starttime=<time>` | 開始時刻下限 |
| `-E`, `--endtime=<time>` | 終了時刻上限 |
| `-o`, `--format=<fields>` | 表示列指定 |
| `-X`, `--allocations` | ステップを省いて割り当て単位表示 |
| `-p`, `--parsable2` | パース向け区切り出力 |
| `-n`, `--noheader` | ヘッダ非表示 |

よく見る列:

- `JobID`
- `JobName`
- `Partition`
- `State`
- `ExitCode`
- `Elapsed`
- `ReqMem`
- `MaxRSS`
- `AllocCPUS`
- `NodeList`

注意:

- `squeue`は現在の状態確認
- `sacct`は終了後の事後解析
- `--state`だけ指定した場合、時間窓の既定値に注意が必要

## 13. `sinfo`でのクラスタ状態確認

`sinfo`はパーティションとノードの状態を見るコマンドです。

例:

```bash
sinfo
sinfo -p gpu
sinfo --long
sinfo -N
sinfo -o "%20P %10a %10l %6D %10T %N"
```

よく使うオプション:

| オプション | 意味 |
| --- | --- |
| `-p`, `--partition=<name>` | パーティション絞り込み |
| `-N`, `--Node` | ノード単位で表示 |
| `-l`, `--long` | 詳細表示 |
| `-o`, `--format=<fmt>` | 出力フォーマット指定 |
| `-t`, `--states=<states>` | ノード状態絞り込み |
| `-h`, `--noheader` | ヘッダ非表示 |
| `-r`, `--responding` | 応答ノードのみ |

見たい情報:

- パーティション名
- 利用可否
- タイムリミット
- ノード数
- ノード状態
- GRES/GPU関連情報

## 14. ジョブ状態コード

よく見る状態:

| 状態 | 意味 |
| --- | --- |
| `PENDING` | 待機中 |
| `RUNNING` | 実行中 |
| `COMPLETED` | 正常終了 |
| `FAILED` | 異常終了 |
| `CANCELLED` | キャンセルされた |
| `TIMEOUT` | 制限時間超過 |
| `OUT_OF_MEMORY` | メモリ不足 |
| `NODE_FAIL` | ノード障害 |
| `PREEMPTED` | プリエンプトされた |
| `SUSPENDED` | 一時停止 |
| `COMPLETING` | 終了処理中 |
| `CONFIGURING` | 構成中 |

`squeue`では略号で出ることがあります。

| 略号 | 状態 |
| --- | --- |
| `PD` | `PENDING` |
| `R` | `RUNNING` |
| `CG` | `COMPLETING` |
| `CD` | `COMPLETED` |
| `CA` | `CANCELLED` |
| `F` | `FAILED` |
| `TO` | `TIMEOUT` |
| `OOM` | `OUT_OF_MEMORY` |

## 15. Slurmが設定する主な環境変数

ジョブ内でよく参照する変数:

| 変数 | 意味 |
| --- | --- |
| `SLURM_JOB_ID` | jobid |
| `SLURM_JOB_NAME` | ジョブ名 |
| `SLURM_JOB_NODELIST` | 割り当てノード一覧 |
| `SLURM_NNODES` | ノード数 |
| `SLURM_NTASKS` | タスク数 |
| `SLURM_CPUS_PER_TASK` | タスクあたりCPU数 |
| `SLURM_SUBMIT_DIR` | 投入時カレントディレクトリ |
| `SLURM_SUBMIT_HOST` | 投入元ホスト |
| `SLURM_ARRAY_JOB_ID` | 配列親jobid |
| `SLURM_ARRAY_TASK_ID` | 配列インデックス |
| `SLURM_PROCID` | タスクの相対番号 |
| `SLURM_LOCALID` | ノード内ローカルタスク番号 |
| `SLURM_NODEID` | ジョブ内ノード番号 |

例:

```bash
echo "$SLURM_JOB_ID"
echo "$SLURM_ARRAY_TASK_ID"
echo "$SLURM_CPUS_PER_TASK"
```

## 16. 典型的なジョブスクリプト例

### CPU並列

```bash
#!/bin/bash
#SBATCH -J cpu-job
#SBATCH -p cpu
#SBATCH -n 1
#SBATCH -c 8
#SBATCH --mem=16G
#SBATCH -t 02:00:00
#SBATCH -o logs/%x-%j.out

export OMP_NUM_THREADS="$SLURM_CPUS_PER_TASK"
srun ./app
```

### MPI

```bash
#!/bin/bash
#SBATCH -J mpi-job
#SBATCH -N 2
#SBATCH -n 32
#SBATCH -t 01:00:00
#SBATCH -p cpu

srun --mpi=pmix ./mpi_app
```

### GPU

```bash
#!/bin/bash
#SBATCH -J gpu-job
#SBATCH -p gpu
#SBATCH --gpus=1
#SBATCH -c 8
#SBATCH --mem=32G
#SBATCH -t 04:00:00
#SBATCH -o logs/%j.out
#SBATCH -e logs/%j.err

srun python train.py
```

### 配列ジョブ

```bash
#!/bin/bash
#SBATCH -J sweep
#SBATCH --array=0-31%4
#SBATCH -t 00:30:00
#SBATCH -o logs/%A_%a.out

python run_case.py --index "$SLURM_ARRAY_TASK_ID"
```

### 依存ジョブ

```bash
jid1=$(sbatch --parsable preprocess.sh)
jid2=$(sbatch --parsable --dependency=afterok:"$jid1" train.sh)
sbatch --dependency=afterok:"$jid2" evaluate.sh
```

## 17. 実務上の注意点

- `sbatch`は投入コマンドであり、その場で処理が走るとは限らない
- `#SBATCH`はスクリプト冒頭にまとめるべき
- `--ntasks`と`--cpus-per-task`は役割が違う
- ログは`--output`と`--error`を明示した方が追跡しやすい
- 失敗解析では`squeue`だけでなく`sacct`を見る
- `--mem`か`--mem-per-cpu`かは運用ルールに合わせる
- `account`、`qos`、`partition`、`constraint`、GPU表記はクラスタ依存
- 公式オプションがクラスタで禁止されている場合もある
- 詳細確認には`scontrol show job <jobid>`が有効
- 実際の運用はサイトローカルルールが最終的に支配する
