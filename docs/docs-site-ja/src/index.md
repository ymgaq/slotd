# slotd

![slotd](/slotd/assets/slotd.png)

`slotd` は、Rust で実装された単一ノード・単一ユーザー向けの Slurm 風スケジューラです。

他言語のドキュメント:
- [English](https://ymgaq.github.io/slotd/)
- [中文版](https://ymgaq.github.io/slotd/cn/)

対象はクラスタではなく 1 台のワークステーションです。一般的な Slurm のコマンド名と主要オプションを残しつつ、実行モデルは意図的に単純化しています。

- 1 つのローカル daemon
- 1 つの SQLite データベース
- 1 台の実行ホスト
- 1 人のローカルユーザー向けワークフロー

利用するコマンドは次のとおりです。

- `sbatch`
- `srun`
- `salloc`
- `squeue`
- `sacct`
- `scontrol`
- `scancel`
- `sinfo`

## 向いている用途

`slotd` は次の用途に向いています。

- ローカル実験ジョブの待ち行列管理
- 長時間実行する CPU / GPU ジョブ
- 1 台のマシン上でのバッチ処理
- リソース確保付きの対話実行
- ワークステーション上での軽量な Slurm 風インターフェース

次の用途は対象外です。

- マルチノードスケジューリング
- クラスタ管理
- account、QoS、fairshare
- ホストをまたぐ federation や reservation

## 主な特徴

- 単一の Rust バイナリとして実装
- daemon と Unix domain socket を使用
- 状態は SQLite に永続化
- CPU、メモリ、GPU の予約を管理
- バッチジョブ、配列ジョブ、対話実行、allocation、step をサポート
- 遅延開始、1 回だけの requeue、dependency、ローカル feature constraint をサポート

## このドキュメントに含まれる内容

- [インストール](installation.md)
- [クイックスタート](quickstart.md)
- [ランタイムモデル](runtime-model.md)
- [`sbatch` によるバッチジョブ](sbatch.md)
- [`srun` による対話実行](srun.md)
- [`salloc` によるアロケーション](salloc.md)
- [キュー状態と集計情報](queue-and-accounting.md)
- [ジョブ制御](job-control.md)
- [ノードとパーティション表示](sinfo.md)
- [テスト](testing.md)
- [実行例](examples.md)
- [トラブルシューティング](troubleshooting.md)
