# Slurmユーザー向けコマンド・オプション・`#SBATCH`ディレクティブ総覧

この文書は、Slurm 25.11 の SchedMD 公式ドキュメントに基づき、一般利用者が参照するコマンド群、主要オプション、`sbatch`/`srun`/`salloc` の資源指定モデル、監視・会計・制御系コマンド、ジョブ状態、主要環境変数を整理したものです。`slotd` との差分や実装状況は考慮しません。

重要な前提:

- Slurm はサイトローカル設定の影響が非常に大きい
- 公式に存在するオプションでも、クラスタ設定により無効化・制限・既定値変更されうる
- `account`、`qos`、`partition`、`constraint`、`gres`、GPU 名、会計項目、メール通知、コンテナ機能などはサイト設定依存
- 本書は公式仕様を整理したものであり、最終的な挙動は対象クラスタの運用ポリシーが優先される

参照元:

- [Slurm Manual Index](https://slurm.schedmd.com/man_index.html)
- [Overview](https://slurm.schedmd.com/overview.html)
- [Quick Start User Guide](https://slurm.schedmd.com/quickstart.html)
- [CPU Management](https://slurm.schedmd.com/cpu_management.html)
- [Heterogeneous Jobs](https://slurm.schedmd.com/heterogeneous_jobs.html)
- [Job State Codes](https://slurm.schedmd.com/job_state_codes.html)
- [`sbatch`](https://slurm.schedmd.com/sbatch.html)
- [`srun`](https://slurm.schedmd.com/srun.html)
- [`salloc`](https://slurm.schedmd.com/salloc.html)
- [`squeue`](https://slurm.schedmd.com/squeue.html)
- [`scancel`](https://slurm.schedmd.com/scancel.html)
- [`sacct`](https://slurm.schedmd.com/sacct.html)
- [`sinfo`](https://slurm.schedmd.com/sinfo.html)
- [`scontrol`](https://slurm.schedmd.com/scontrol.html)
- [`sattach`](https://slurm.schedmd.com/sattach.html)
- [`sbcast`](https://slurm.schedmd.com/sbcast.html)
- [`sprio`](https://slurm.schedmd.com/sprio.html)
- [`sshare`](https://slurm.schedmd.com/sshare.html)
- [`sstat`](https://slurm.schedmd.com/sstat.html)
- [`scrontab`](https://slurm.schedmd.com/scrontab.html)

## 1. Slurm公式マニュアルにあるコマンド一覧

Slurm 25.11 の man index の `Commands` セクションに掲載されているコマンドは次のとおりです。

| コマンド | 公式説明の要点 | 一般ユーザー視点での位置づけ |
| --- | --- | --- |
| `sacct` | 会計ログ / DB からジョブ・ステップ履歴を表示 | 終了後の解析で必須 |
| `sacctmgr` | アカウント情報の閲覧・変更 | 多くは管理者向け |
| `salloc` | ジョブ割り当てを取得し、コマンド実行後に解放 | 対話利用向け |
| `sattach` | ジョブステップへ接続 | 実行中ステップの I/O 接続 |
| `sbatch` | バッチスクリプト投入 | バッチ実行の中心 |
| `sbcast` | 割り当てノードへファイル配布 | 補助ツール |
| `scancel` | ジョブ / ステップへシグナル送信・取消 | 停止・制御 |
| `scontrol` | Slurm の状態・設定の表示 / 変更 | 詳細確認や一部操作 |
| `scrontab` | Slurm crontab 管理 | 定期ジョブ |
| `scrun` | Slurm 用 OCI runtime proxy | 主にコンテナ統合向け |
| `sdiag` | スケジューリング診断 | 管理・診断寄り |
| `sh5util` | `acct_gather_profile` 用ユーティリティ | 補助ツール |
| `sinfo` | ノード・パーティション情報表示 | 資源把握 |
| `sprio` | ジョブ優先度の内訳表示 | 待ち行列解析 |
| `squeue` | キュー中ジョブ表示 | 現在状態の確認で必須 |
| `sreport` | 会計データからレポート生成 | 管理・分析寄り |
| `srun` | 並列ジョブ実行 | 対話 / ジョブステップ起動 |
| `sshare` | association share 表示 | fairshare 確認 |
| `sstat` | 実行中ジョブ / ステップ統計表示 | 実行中解析 |
| `strigger` | trigger 情報の設定 / 取得 / クリア | 管理寄り |
| `sview` | GUI フロントエンド | GUI 利用時 |

一般利用者が日常的に使う中心コマンドは次です。

- `sbatch`
- `srun`
- `salloc`
- `squeue`
- `scancel`
- `sacct`
- `sinfo`
- `scontrol`
- `sstat`
- `sprio`
- `sshare`

## 2. まず押さえるSlurmの実行モデル

Slurm 利用で混乱しやすいのは、`job`、`allocation`、`step` が別概念である点です。

| 概念 | 説明 |
| --- | --- |
| job | Slurm が管理するジョブ単位。`sbatch` や `salloc` や `srun` で作られる |
| allocation | ノード / CPU / メモリ / GRES 等の資源割り当て |
| step | 割り当て内で `srun` が起動する実行単位 |
| batch job | `sbatch` で投入されるバッチスクリプト実行 |
| interactive allocation | `salloc` で先に資源だけ確保する利用形態 |
| array job | 同一定義の複数タスク群を一つのジョブ群として扱う仕組み |
| heterogeneous job | コンポーネントごとに異なる資源要求を持つ複合ジョブ |

典型的な流れ:

1. `sbatch` でバッチ投入する、または `salloc` で資源確保する
2. `squeue` で待ち / 実行状況を見る
3. 実行中は `sstat`、終了後は `sacct` で結果を見る
4. 必要なら `scancel` で停止する
5. 深掘りは `scontrol show job <jobid>`、優先度解析は `sprio`、fairshare は `sshare`

## 3. `sbatch` / `#SBATCH` / 環境変数の優先順位

`sbatch` は CLI オプション、スクリプト内 `#SBATCH` ディレクティブ、`SBATCH_*` 環境変数の三層を持ちます。公式には次の順で優先されます。

1. `sbatch` コマンドライン引数
2. 環境変数
3. `#SBATCH` ディレクティブ

つまり、既存のシェル環境で `SBATCH_PARTITION=gpu` などを設定していると、スクリプト内指定よりそちらが優先されます。

## 4. `#SBATCH`ディレクティブの公式仕様

`#SBATCH` は `sbatch` オプションをスクリプト冒頭のコメント領域に書く仕組みです。

重要な規則:

- `#SBATCH` で始まる行のみ解釈される
- 先頭の連続したコメント / 空行 / shebang 領域のみが対象
- 最初の「空行でもコメントでもない行」に到達すると、その後ろの `#SBATCH` は無視される
- `#SBATCH` 行はシェル構文ではなく、シェル変数展開やコマンド置換はされない
- コマンドライン引数が同一指定を上書きする

有効な例:

```bash
#!/bin/bash
#SBATCH --job-name=train
#SBATCH --partition=gpu
#SBATCH --gpus=1

set -euo pipefail
srun python train.py
```

無効化される例:

```bash
#!/bin/bash
echo start
#SBATCH --time=10:00
```

## 5. `sbatch`の役割と基本仕様

`sbatch` はバッチスクリプトを Slurm へ登録するコマンドです。成功するとジョブ ID を返し、実行は後続のスケジューリングに委ねられます。

基本形:

```bash
sbatch [options] script [args...]
```

標準入力からも投入できます。

```bash
sbatch <<'EOF'
#!/bin/bash
hostname
EOF
```

1 行コマンドなら `--wrap` が使えます。

```bash
sbatch --wrap="python train.py --epochs 10"
```

補足:

- `sbatch` 自体は通常すぐ終了する
- 返るのは投入結果であり、実行完了ではない
- `-W`, `--wait` を使うとジョブ終了まで待てる
- `--parsable` は自動処理向けの出力に便利
- `sbatch` は heterogeneous jobs を `:` 区切りで表現できる

## 6. `sbatch`オプション総覧

`sbatch` のオプションは非常に多いので、公式 man page に沿ってカテゴリ別に整理します。`srun` / `salloc` と共有されるものが多く、特に資源要求系・配置系・環境系は三者で共通性があります。

### 6.1 識別・メタデータ

| オプション | 内容 |
| --- | --- |
| `-J`, `--job-name=<name>` | ジョブ名 |
| `--comment=<string>` | コメント文字列 |
| `--wckey=<key>` | workload characterization key |
| `--mcs-label=<label>` | MCS ラベル |
| `--nice[=<adjustment>]` | 優先度調整 |
| `--priority=<value>` | 優先度指定 |

### 6.2 会計・所属・キュー属性

| オプション | 内容 |
| --- | --- |
| `-A`, `--account=<account>` | 会計アカウント |
| `-p`, `--partition=<names>` | パーティション |
| `--qos=<qos>` | QoS |
| `--reservation=<name>` | 予約済み資源の利用 |
| `-M`, `--clusters=<names>` | 対象クラスタ |
| `--licenses=<spec>` | ライセンス要求 |
| `--profile=<types>` | profiling / acct_gather_profile |

### 6.3 ノード・タスク・CPU 数

| オプション | 内容 |
| --- | --- |
| `-N`, `--nodes=<min[-max]>` | ノード数 |
| `-n`, `--ntasks=<number>` | 総タスク数 |
| `-c`, `--cpus-per-task=<n>` | タスクあたり CPU 数 |
| `--cpus-per-gpu=<n>` | GPU あたり CPU 数 |
| `--ntasks-per-node=<n>` | ノードあたりタスク数 |
| `--ntasks-per-socket=<n>` | ソケットあたりタスク数 |
| `--ntasks-per-core=<n>` | コアあたりタスク数 |
| `--ntasks-per-gpu=<n>` | GPU あたりタスク数 |
| `--mincpus=<n>` | ノードごとの最小 CPU 数 |
| `--sockets-per-node=<n>` | ノードごとのソケット数制約 |
| `--cores-per-socket=<n>` | ソケットごとのコア数制約 |
| `--threads-per-core=<n>` | コアごとのスレッド数制約 |
| `--hint=<type>` | SMT / binding ヒント |
| `--extra-node-info=<sockets[:cores[:threads]]>` | 追加トポロジ制約 |

考え方:

- `--ntasks` は MPI ランク数やプロセス数に近い
- `--cpus-per-task` は 1 タスクが専有する CPU 数
- OpenMP 系は `--cpus-per-task`
- MPI 系は `--ntasks`
- ハイブリッドは両方明示する

### 6.4 メモリ・一時ディスク

| オプション | 内容 |
| --- | --- |
| `--mem=<size>` | ノードあたりメモリ |
| `--mem-per-cpu=<size>` | CPU あたりメモリ |
| `--mem-per-gpu=<size>` | GPU あたりメモリ |
| `--tmp=<size>` | ノードあたり一時ディスク量 |
| `--mem-bind=<type>` | NUMA / メモリバインド |

補足:

- `--mem`、`--mem-per-cpu`、`--mem-per-gpu` は意図的に使い分ける
- 単位は通常 `K`, `M`, `G`, `T`
- 既定単位や上限はクラスタ設定依存

### 6.5 GPU・GRES・TRES

| オプション | 内容 |
| --- | --- |
| `-G`, `--gpus=[type:]<number>` | 総 GPU 数 |
| `--gpus-per-node=[type:]<number>` | ノードあたり GPU 数 |
| `--gpus-per-socket=[type:]<number>` | ソケットあたり GPU 数 |
| `--gpus-per-task=[type:]<number>` | タスクあたり GPU 数 |
| `--gpu-bind=[verbose,]<type>` | GPU バインド |
| `--gpu-freq=[<type]=value>[,verbose]` | GPU 周波数要求 |
| `--gres=<name[:type]:count>` | generic resources |
| `--gres-flags=<opts>` | GRES の割り当て挙動 |
| `--tres-bind=<tres>:[verbose,]<type>` | TRES バインド |
| `--tres-per-task=<tres_spec>` | タスクあたり TRES |

補足:

- 公式 docs では GPU 指定と GRES 指定が併存する
- GPU オプションの解釈は `select/cons_tres`、`gres.conf`、サイト設定に依存
- `--gpu-bind` は内部的に `--tres-bind=gres/gpu:...` と対応する

### 6.6 ノード選択・配置・共有

| オプション | 内容 |
| --- | --- |
| `-w`, `--nodelist=<nodes>` | 利用ノード指定 |
| `-x`, `--exclude=<nodes>` | 除外ノード指定 |
| `-C`, `--constraint=<features>` | feature 制約 |
| `--prefer=<features>` | 希望 feature |
| `--contiguous` | 連続ノード要求 |
| `-m`, `--distribution=<spec>` | タスク分布方式 |
| `--exclusive[=<user|mcs|topo>]` | 排他割り当て |
| `--oversubscribe` | over-subscribe を許容 |
| `--overcommit` | CPU overcommit |
| `--spread-job` | ジョブを広く分散 |
| `--switches[=<count>[@max-time]]` | ネットワークトポロジ制約 |
| `--network=<spec>` | ネットワーク要求 |
| `--core-spec=<n>` | specialized cores |
| `--thread-spec=<n>` | specialized threads |
| `--delay-boot=<minutes>` | reboot を伴うノード選定の遅延 |
| `--no-kill` | ノード障害時に全体終了しない |

### 6.7 時刻・期間・再投入

| オプション | 内容 |
| --- | --- |
| `-t`, `--time=<time>` | 制限時間 |
| `--time-min=<time>` | 許容最小時間 |
| `--begin=<time>` | 開始抑止時刻 |
| `--deadline=<time>` | この時刻までに開始できなければ不適格 |
| `--requeue` | 再キュー許可 |
| `--no-requeue` | 再キュー禁止 |
| `--signal=<spec>` | 終了前シグナル送信 |

時間指定の代表形式:

- `minutes`
- `minutes:seconds`
- `hours:minutes:seconds`
- `days-hours`
- `days-hours:minutes`
- `days-hours:minutes:seconds`

### 6.8 依存関係・保留・即時性

| オプション | 内容 |
| --- | --- |
| `-d`, `--dependency=<spec>` | 他ジョブ依存 |
| `--kill-on-invalid-dep=<yes|no>` | 不正依存の扱い |
| `-H`, `--hold` | 保留状態で投入 |
| `--hold` | 同上 |
| `-I`, `--immediate[=<seconds>]` | 即時割り当て不可なら失敗 |
| `--test-only` | 実投入せず検証 |

代表的依存種別:

- `after:<jobid>`
- `afterany:<jobid>`
- `afterok:<jobid>`
- `afternotok:<jobid>`
- `singleton`
- `aftercorr:<jobid>`

### 6.9 配列ジョブ

| オプション | 内容 |
| --- | --- |
| `-a`, `--array=<indexes>` | array job |

代表的書式:

- `0-9`
- `1,3,5`
- `0-31:2`
- `0-99%8`

関連環境変数:

- `SLURM_ARRAY_JOB_ID`
- `SLURM_ARRAY_TASK_ID`
- `SLURM_ARRAY_TASK_COUNT`
- `SLURM_ARRAY_TASK_MIN`
- `SLURM_ARRAY_TASK_MAX`

### 6.10 入出力・作業ディレクトリ

| オプション | 内容 |
| --- | --- |
| `-o`, `--output=<pattern>` | 標準出力 |
| `-e`, `--error=<pattern>` | 標準エラー |
| `-i`, `--input=<path>` | 標準入力 |
| `--open-mode=append|truncate` | 既存ファイルへの追記 / 上書き |
| `-D`, `--chdir=<dir>` | 実行前にディレクトリ変更 |
| `--wait-all-nodes=<0|1>` | 全ノード起動待ち制御 |

### 6.11 環境・ユーザー・資格情報

| オプション | 内容 |
| --- | --- |
| `--export=<spec>` | 環境変数エクスポート制御 |
| `--export-file=<file|fd>` | 環境変数ファイル |
| `--get-user-env[=<timeout>]` | ログイン環境取得 |
| `--uid=<uid>` | 実行 UID |
| `--gid=<gid>` | 実行 GID |
| `--propagate[=<rlimits>]` | resource limits 伝播 |

代表例:

- `--export=ALL`
- `--export=NONE`
- `--export=ALL,OMP_NUM_THREADS=8`
- `--export=VAR1,VAR2`

### 6.12 通知・可観測性・補助

| オプション | 内容 |
| --- | --- |
| `--mail-type=<types>` | メール通知タイミング |
| `--mail-user=<addr>` | 通知先 |
| `--acctg-freq=<datatype>=<interval>` | accounting frequency |
| `--parsable` | parse 向け jobid 出力 |
| `-W`, `--wait` | 終了まで待つ |
| `--wrap=<command>` | 1 行コマンド投入 |
| `-v`, `--verbose` | 詳細化 |
| `--quiet` | 出力抑制 |
| `--usage` | 簡易 usage |
| `-V`, `--version` | version |
| `--help` | help |

### 6.13 バーストバッファ・電力・コンテナ等の拡張機能

公式 man page には、サイトが機能を有効化している場合に利用できる拡張オプションも含まれます。

| オプション | 内容 |
| --- | --- |
| `--bb=<spec>` | burst buffer 要求 |
| `--bbf=<file>` | burst buffer spec file |
| `--power=<flags>` | power management |
| `--container=<bundle>` | OCI bundle |
| `--container-id=<id>` | container id |
| `--batch=<features>` | batch host 制約 |
| `--reboot` | ノード reboot 許容 |

## 7. `sbatch`出力ファイル名パターン

`--output` / `--error` では置換パターンを使えます。よく使うものは次です。

| パターン | 意味 |
| --- | --- |
| `%j` | jobid |
| `%J` | `jobid.stepid` |
| `%A` | array 親 jobid |
| `%a` | array task index |
| `%N` | short hostname |
| `%n` | ジョブ内ノード番号 |
| `%t` | task 番号 |
| `%u` | user 名 |
| `%x` | job 名 |
| `%%` | リテラル `%` |

代表例:

```bash
sbatch -o logs/%x-%j.out -e logs/%x-%j.err job.sh
sbatch --array=0-31 -o logs/%A_%a.out job.sh
```

## 8. `srun`の役割とオプション

`srun` は二つの文脈で使われます。

1. その場でジョブを起動する
2. 既存 allocation 内で job step を起動する

基本形:

```bash
srun [options...] executable [args...]
```

例:

```bash
srun -n 4 hostname
salloc -N 1 -n 4
srun ./app
srun --pty bash
```

`srun` は資源要求系の大半を `sbatch` / `salloc` と共有します。加えて、job step 起動コマンドとして次のオプションが特に重要です。

| オプション | 内容 |
| --- | --- |
| `--pty` | 疑似端末つき対話起動 |
| `--mpi=<type>` | MPI 起動方式 |
| `--cpu-bind=<type>` | CPU binding |
| `--mem-bind=<type>` | memory binding |
| `--gpu-bind=<type>` | GPU binding |
| `--label` | 各行に task id ラベル付与 |
| `-l`, `--label` | 同上 |
| `-u`, `--unbuffered` | 標準出力 / エラーを行単位バッファなし |
| `-K`, `--kill-on-bad-exit[=<0|1>]` | どれかの task が失敗したら残りも止める |
| `--overlap` | 他 step と資源共有可 |
| `--exact` | 正確な step 資源使用 |
| `--relative=<n>` | allocation 内相対ノード指定 |
| `--multi-prog` | 複数実行プログラム構成 |
| `--bcast[=<dest_path>]` | 実行ファイル配布 |
| `--send-libs[=<yes|no>]` | 依存ライブラリ転送 |
| `--slurmd-debug=<level>` | `slurmd` 側 debug |
| `--task-epilog=<file>` | task epilog |
| `--task-prolog=<file>` | task prolog |
| `--quit-on-interrupt` | SIGINT で即終了 |
| `--preserve-env` | ローカル環境維持 |
| `--resv-ports` | 通信ポート予約 |
| `--wait=<sec>` | task 終了待ちの制御 |

実務上の典型:

- `srun --pty bash`
- `srun -N 2 -n 32 --mpi=pmix ./mpi_app`
- `srun -c 8 --cpu-bind=cores ./omp_app`
- `srun --label -n 4 hostname`

## 9. `salloc`の役割とオプション

`salloc` は「資源だけを先に確保する」コマンドです。確保後、そのシェルまたは指定コマンドの中で `srun` を使います。

基本形:

```bash
salloc [options...] [command [args...]]
```

例:

```bash
salloc -N 1 -n 4 -t 01:00:00
srun --pty bash
```

`salloc` は資源要求オプションの多くを `sbatch` と共有し、対話用途向けに次がよく使われます。

| オプション | 内容 |
| --- | --- |
| `--no-shell` | shell を起動しない |
| `--bell` | 割り当て成立時にベル |
| `--immediate[=<seconds>]` | 即時に確保できなければ失敗 |
| `--label` | 出力ラベル |

用途:

- 対話デバッグ
- Jupyter など長寿命プロセス前段
- ノード上での手動検証
- 複数回 `srun` を投げるセッション

## 10. キュー監視系コマンド

### 10.1 `squeue`

`squeue` は現在キューに存在するジョブを表示します。待機中・実行中の確認に使います。

基本例:

```bash
squeue
squeue -u "$USER"
squeue --jobs 12345,12346
squeue --states=PENDING,RUNNING
squeue --format="%.18i %.9P %.20j %.8u %.2t %.10M %.6D %R"
```

主要オプション:

| オプション | 内容 |
| --- | --- |
| `-a`, `--all` | すべて表示 |
| `-r`, `--array` | 配列要素を展開 |
| `-h`, `--noheader` | ヘッダ非表示 |
| `-j`, `--jobs=<ids>` | jobid 指定 |
| `-u`, `--user=<users>` | user 指定 |
| `-p`, `--partition=<parts>` | partition 指定 |
| `-t`, `--states=<states>` | state 指定 |
| `-n`, `--name=<names>` | job name 指定 |
| `-w`, `--nodelist=<nodes>` | node 指定 |
| `-o`, `--format=<fmt>` | format 指定 |
| `-O`, `--Format=<fmt>` | long field names を使う format |
| `-l`, `--long` | long format |
| `-s`, `--steps` | steps 表示 |
| `-S`, `--sort=<spec>` | sort 指定 |
| `--start` | 推定開始時刻つき表示 |
| `--me` | 現ユーザーに限定 |
| `-M`, `--clusters=<names>` | 複数クラスタ |
| `--yaml`, `--json` | machine readable 出力 |

`NODELIST(REASON)` は重要です。

- 実行中ならノード一覧
- 待機中なら理由
- 代表的理由は `Priority`, `Resources`, `Dependency`, `ReqNodeNotAvail`

### 10.2 `sprio`

`sprio` はジョブ優先度の内訳を表示します。

主な用途:

- なぜ待っているかを優先度面から見る
- age / fairshare / job size / QOS などの構成要素を見る

代表的オプション:

| オプション | 内容 |
| --- | --- |
| `-j`, `--jobs=<ids>` | jobid 指定 |
| `-u`, `--users=<users>` | user 指定 |
| `-o`, `--format=<fmt>` | format 指定 |
| `-l`, `--long` | 詳細表示 |
| `-M`, `--clusters=<names>` | 複数クラスタ |
| `-n`, `--noheader` | ヘッダなし |

### 10.3 `sshare`

`sshare` は association / fairshare 情報を表示します。

主な用途:

- 自分の fairshare や usage の確認
- account / user association の share 確認

代表的オプション:

| オプション | 内容 |
| --- | --- |
| `-A`, `--accounts=<accounts>` | account 指定 |
| `-u`, `--users=<users>` | user 指定 |
| `-a`, `--all` | すべて表示 |
| `-l`, `--long` | 長い形式 |
| `-o`, `--format=<fmt>` | format 指定 |
| `-n`, `--noheader` | ヘッダなし |
| `-P`, `--parsable2` | parse 向け |
| `-M`, `--clusters=<names>` | 複数クラスタ |

## 11. 会計・統計コマンド

### 11.1 `sacct`

`sacct` は終了済みを含むジョブ履歴を表示します。ジョブ会計 DB または accounting log に依存します。

基本例:

```bash
sacct -j 12345
sacct -j 12345 --format=JobID,JobName,Partition,State,ExitCode,Elapsed,MaxRSS
sacct -s FAILED,TIMEOUT
sacct -S 2026-04-01 -E 2026-04-11
```

主要オプション:

| オプション | 内容 |
| --- | --- |
| `-A`, `--accounts=<accounts>` | account 指定 |
| `--array` | array task を展開 |
| `-L`, `--allclusters` | 全クラスタ |
| `-X`, `--allocations` | allocation 単位のみ |
| `-a`, `--allusers` | 全ユーザー |
| `-j`, `--jobs=<ids>` | jobid 指定 |
| `-s`, `--state=<states>` | state 指定 |
| `-S`, `--starttime=<time>` | 開始時刻下限 |
| `-E`, `--endtime=<time>` | 終了時刻上限 |
| `-o`, `--format=<fields>` | 出力列 |
| `-n`, `--noheader` | ヘッダなし |
| `-p`, `-P` | parse 向け出力 |
| `-u`, `--user=<users>` | user 指定 |
| `-M`, `--clusters=<names>` | cluster 指定 |
| `-b`, `--brief` | 簡潔表示 |
| `-D`, `--duplicates` | 重複 jobid 表示 |
| `-e`, `--helpformat` | 利用可能 field 一覧 |
| `-k`, `--timelimit-min=<time>` | minimum timelimit |
| `-K`, `--timelimit-max=<time>` | maximum timelimit |
| `-q`, `--qos=<qos_list>` | qos 指定 |
| `-r`, `--partition=<parts>` | partition 指定 |
| `-T`, `--truncate` | 時刻範囲で集計を切る |

よく見る列:

- `JobID`
- `JobName`
- `Partition`
- `Account`
- `AllocCPUS`
- `State`
- `ExitCode`
- `Elapsed`
- `ReqMem`
- `MaxRSS`
- `NodeList`

### 11.2 `sstat`

`sstat` は実行中ジョブ / step の統計を表示します。`jobacct_gather` が必要です。

基本例:

```bash
sstat -j 12345.batch
sstat -j 12345.0 --format=JobID,AveCPU,MaxRSS,MaxVMSize
```

主要オプション:

| オプション | 内容 |
| --- | --- |
| `-a`, `--allsteps` | step をまとめて表示 |
| `-j`, `--jobs=<job.step>` | job/step 指定 |
| `-o`, `--format`, `--fields` | 表示 field |
| `--helpformat` | 利用可能 field 一覧 |
| `-i`, `--pidformat` | pid field 指定 |
| `-n`, `--noheader` | ヘッダなし |
| `-p`, `--parsable` | parse 向け |
| `-P`, `--parsable2` | parse 向け |
| `--usage` | usage |
| `-V`, `--version` | version |

## 12. ノード・パーティション確認

### `sinfo`

`sinfo` はパーティションとノードの状態を見るコマンドです。

基本例:

```bash
sinfo
sinfo -p gpu
sinfo -N
sinfo --long
sinfo -o "%20P %10a %10l %6D %10T %N"
```

主要オプション:

| オプション | 内容 |
| --- | --- |
| `-a`, `--all` | hidden partition も含める |
| `-N`, `--Node` | node 単位表示 |
| `-p`, `--partition=<parts>` | partition 指定 |
| `-n`, `--nodes=<nodes>` | nodes 指定 |
| `-t`, `--states=<states>` | node state 指定 |
| `-r`, `--responding` | 応答ノードのみ |
| `-d`, `--dead` | 応答なしノードのみ |
| `-e`, `--exact` | まとめず正確表示 |
| `-h`, `--noheader` | ヘッダなし |
| `-l`, `--long` | long format |
| `-o`, `--format=<fmt>` | format |
| `-O`, `--Format=<fmt>` | long field names format |
| `-R`, `--list-reasons` | drain/down 理由一覧 |
| `-s`, `--summarize` | サマリ形式 |
| `-S`, `--sort=<spec>` | sort |
| `-M`, `--clusters=<names>` | cluster 指定 |
| `--yaml`, `--json` | machine readable 出力 |

## 13. 制御・取消・詳細参照

### 13.1 `scancel`

`scancel` はジョブや job step にシグナルを送るコマンドです。

基本例:

```bash
scancel 12345
scancel 12345_7
scancel --signal=TERM 12345
scancel --user="$USER" --state=PENDING
```

主要オプション:

| オプション | 内容 |
| --- | --- |
| `-A`, `--account=<accounts>` | account 指定 |
| `-b`, `--batch` | batch step のみに送る |
| `-f`, `--full` | batch step と子 step にも送る |
| `-i`, `--interactive` | 確認付き |
| `-n`, `--name=<names>` | job name 指定 |
| `-p`, `--partition=<parts>` | partition 指定 |
| `-q`, `--qos=<qos>` | qos 指定 |
| `-R`, `--reservation=<name>` | reservation 指定 |
| `-s`, `--signal=<sig>` | シグナル指定 |
| `-t`, `--state=<states>` | state 指定 |
| `-u`, `--user=<users>` | user 指定 |
| `-w`, `--nodelist=<nodes>` | node 指定 |
| `-M`, `--clusters=<names>` | cluster 指定 |
| `--ctld` | `slurmctld` 経由で処理 |
| `--sibling=<cluster>` | federated sibling 指定 |
| `--quiet` | quiet |
| `--verbose` | verbose |

注意:

- array task は `jobid_taskid` 形式で個別指定できる
- 条件付き cancel は誤爆防止のため `squeue` と併用すべき
- `--signal` を使えば kill だけでなく checkpoint 通知にも使える

### 13.2 `scontrol`

`scontrol` は汎用の表示 / 管理コマンドです。一般ユーザー視点では「詳細確認」が主用途です。

よく使う表示系:

```bash
scontrol show job 12345
scontrol show node node01
scontrol show partition gpu
scontrol show hostnames "$SLURM_JOB_NODELIST"
```

一般ユーザーがよく使うサブコマンド:

| サブコマンド | 用途 |
| --- | --- |
| `show job <jobid>` | ジョブ詳細 |
| `show node <name>` | ノード詳細 |
| `show partition <name>` | パーティション詳細 |
| `show hostnames <nodelist>` | nodelist 展開 |
| `hold <jobid>` | ユーザー保留 |
| `release <jobid>` | 保留解除 |
| `requeue <jobid>` | 再キュー |
| `update JobId=<id> ...` | 一部属性変更 |
| `notify <jobid> <msg>` | 通知 |
| `pidinfo <pid>` | PID 対応確認 |

主要グローバルオプション:

- `-a`, `--all`
- `-d`, `--details`
- `-o`, `--oneliner`
- `-M`, `--clusters=<names>`
- `--json`
- `--yaml`

### 13.3 `sattach`

`sattach` は実行中 step の stdout / stderr へ接続するコマンドです。

基本形:

```bash
sattach [options] <jobid.stepid>
```

主なオプション:

| オプション | 内容 |
| --- | --- |
| `--input-filter=<taskid>` | stdin 対象 task |
| `--output-filter=<taskid>` | stdout 対象 task |
| `--error-filter=<taskid>` | stderr 対象 task |
| `-l`, `--label` | 出力ラベル |
| `--layout` | task layout 表示 |
| `--pty` | pty モード |
| `-Q`, `--quiet` | quiet |
| `-v`, `--verbose` | verbose |

### 13.4 `sbcast`

`sbcast` は allocation ノード群へファイルを配布します。

基本形:

```bash
sbcast [options] source dest
```

主なオプション:

| オプション | 内容 |
| --- | --- |
| `-C`, `--compress[=<library>]` | 圧縮転送 |
| `-f`, `--force` | 上書き |
| `-F`, `--fanout=<n>` | fanout |
| `-j`, `--jobid=<jobid>` | job 指定 |
| `-p`, `--preserve` | 権限 / 時刻保持 |
| `-s`, `--size=<size>` | block size |
| `-t`, `--timeout=<sec>` | timeout |
| `-v`, `--verbose` | verbose |

## 14. 定期実行・補助・分析系

### 14.1 `scrontab`

`scrontab` は Slurm 版 crontab を管理します。エントリごとに `#SCRON` ディレクティブを使い、`sbatch` の多くのオプションを引き継げます。

主なサブコマンド:

| コマンド | 用途 |
| --- | --- |
| `scrontab -e` | 編集 |
| `scrontab -l` | 一覧 |
| `scrontab -r` | 削除 |
| `scrontab <file>` | file から読み込み |

主なオプション:

| オプション | 内容 |
| --- | --- |
| `-e` | 編集 |
| `-l` | 一覧 |
| `-r` | 削除 |
| `-i` | 削除確認 |
| `-u <user>` | user 指定 |

`#SCRON` について:

- 直後の 1 エントリにだけ適用される
- エントリ間でオプションはリセットされる
- 利用できる資源指定の多くは `sbatch` と同系統

### 14.2 `sreport`

`sreport` は会計 DB からレポートを作るコマンドです。一般利用者は制限されることがあります。

### 14.3 `sacctmgr`

`sacctmgr` は account / user / association 管理用で、通常は管理者向けです。

### 14.4 `sdiag`, `strigger`, `sh5util`, `scrun`, `sview`

- `sdiag`: scheduler 診断
- `strigger`: trigger 管理
- `sh5util`: profiling 補助
- `scrun`: OCI runtime proxy
- `sview`: GUI

これらは存在を知っておく程度で十分なケースが多いです。

## 15. ジョブ状態コード

公式の主な状態:

| 状態 | 意味 |
| --- | --- |
| `PENDING` | 待機中 |
| `RUNNING` | 実行中 |
| `SUSPENDED` | 一時停止 |
| `COMPLETING` | 終了処理中 |
| `COMPLETED` | 正常終了 |
| `CANCELLED` | キャンセル |
| `FAILED` | 異常終了 |
| `TIMEOUT` | time limit 超過 |
| `OUT_OF_MEMORY` | OOM |
| `NODE_FAIL` | ノード障害 |
| `PREEMPTED` | preemption |
| `BOOT_FAIL` | boot 失敗 |
| `DEADLINE` | deadline miss |

`squeue` でよく見る略号:

| 略号 | 状態 |
| --- | --- |
| `PD` | `PENDING` |
| `R` | `RUNNING` |
| `S` | `SUSPENDED` |
| `CG` | `COMPLETING` |
| `CD` | `COMPLETED` |
| `CA` | `CANCELLED` |
| `F` | `FAILED` |
| `TO` | `TIMEOUT` |
| `OOM` | `OUT_OF_MEMORY` |
| `NF` | `NODE_FAIL` |
| `PR` | `PREEMPTED` |
| `BF` | `BOOT_FAIL` |
| `DL` | `DEADLINE` |

## 16. Slurm が設定する主要環境変数

コマンドや文脈により差はありますが、ジョブ / allocation / step で特に重要なのは次です。

| 変数 | 意味 |
| --- | --- |
| `SLURM_JOB_ID` | jobid |
| `SLURM_JOB_NAME` | job 名 |
| `SLURM_JOB_NODELIST` | 割り当てノード一覧 |
| `SLURM_NNODES` | ノード数 |
| `SLURM_NTASKS` | タスク数 |
| `SLURM_CPUS_PER_TASK` | task あたり CPU |
| `SLURM_CPUS_PER_GPU` | GPU あたり CPU |
| `SLURM_MEM_PER_CPU` | CPU あたりメモリ |
| `SLURM_MEM_PER_NODE` | ノードあたりメモリ |
| `SLURM_MEM_PER_GPU` | GPU あたりメモリ |
| `SLURM_GPUS` | GPU 数 |
| `SLURM_GPUS_PER_NODE` | ノードあたり GPU |
| `SLURM_GPUS_PER_TASK` | task あたり GPU |
| `SLURM_SUBMIT_DIR` | 投入時ディレクトリ |
| `SLURM_SUBMIT_HOST` | 投入元ホスト |
| `SLURM_ARRAY_JOB_ID` | array 親 jobid |
| `SLURM_ARRAY_TASK_ID` | array index |
| `SLURM_ARRAY_TASK_COUNT` | array 要素数 |
| `SLURM_ARRAY_TASK_MIN` | 最小 index |
| `SLURM_ARRAY_TASK_MAX` | 最大 index |
| `SLURM_PROCID` | job step 内 rank |
| `SLURM_LOCALID` | node 内 local rank |
| `SLURM_NODEID` | allocation 内 node index |
| `SLURM_STEP_ID` | step id |
| `SLURM_STEP_NUM_TASKS` | step task 数 |
| `SLURM_CLUSTER_NAME` | cluster 名 |
| `SLURM_DISTRIBUTION` | 指定 distribution |
| `SLURM_GPU_BIND` | GPU bind 指定 |
| `SLURM_CPU_FREQ_REQ` | CPU frequency 要求 |
| `SLURM_CONTAINER` | OCI bundle |
| `SLURM_CONTAINER_ID` | OCI container id |

## 17. 典型的なジョブスクリプト例

### CPU 並列

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

## 18. 実務で特に重要な整理

- `sbatch` は「投入」であって「即実行」ではない
- `salloc` は allocation、`srun` は step 起動、`sbatch` は batch job
- `#SBATCH` はスクリプト冒頭コメント領域でのみ有効
- 優先順位は `CLI > 環境変数 > #SBATCH`
- `--ntasks` と `--cpus-per-task` は全く別物
- 実行中は `squeue` / `sstat`、終了後は `sacct`
- 詳細確認は `scontrol show job <jobid>`
- 待ち理由の解析は `squeue` の reason と `sprio` の両方を見る
- fairshare が効くクラスタでは `sshare` が効く
- 公式オプションが存在してもサイト設定により使用不可のことがある

## 19. 公式参照先の使い分け

- バッチ投入仕様: [`sbatch`](https://slurm.schedmd.com/sbatch.html)
- 対話 / step 実行: [`srun`](https://slurm.schedmd.com/srun.html), [`salloc`](https://slurm.schedmd.com/salloc.html)
- 現在状態: [`squeue`](https://slurm.schedmd.com/squeue.html), [`sinfo`](https://slurm.schedmd.com/sinfo.html)
- 終了後解析: [`sacct`](https://slurm.schedmd.com/sacct.html), [`sstat`](https://slurm.schedmd.com/sstat.html)
- 制御 / 詳細: [`scancel`](https://slurm.schedmd.com/scancel.html), [`scontrol`](https://slurm.schedmd.com/scontrol.html)
- 優先度 / fairshare: [`sprio`](https://slurm.schedmd.com/sprio.html), [`sshare`](https://slurm.schedmd.com/sshare.html)
- 補助: [`sattach`](https://slurm.schedmd.com/sattach.html), [`sbcast`](https://slurm.schedmd.com/sbcast.html), [`scrontab`](https://slurm.schedmd.com/scrontab.html)
