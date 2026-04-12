# テスト

`slotd` の検証は、主に `tests/` 配下の Rust 統合テストで行っています。
各テストは一時ディレクトリ上に分離されたランタイムを作成し、専用 daemon を起動したうえで、公開されている Slurm 風コマンドをコンパイル済みの `slotd` バイナリ経由で実行します。
そのため、普段使っている `SLOTD_ROOT` を汚さずに確認できます。

## 実行方法

テスト全体を実行:

```bash
cargo test
```

特定の統合テストファイルだけを実行:

```bash
cargo test --test scheduling
```

特定のテストケースだけを実行:

```bash
cargo test dependency_job_waits_for_prerequisite_before_running --test scheduling
```

## 何を検証しているか

現在のテストスイートは、利用者に影響する次の挙動を中心にカバーしています。

- `sbatch`、`srun`、`salloc`、`sinfo`、`squeue`、`sacct`、`scontrol` の基本フロー
- dependency、配列ジョブ、遅延開始、constraint、resource flag、requeue などのスケジューリング規則
- `srun --pty`、`--label`、`--unbuffered`、allocation / step などの対話実行・前景実行パス
- cancellation、warning signal、update 処理、出力ファイル配置などのライフサイクルと回復処理
- `SLOTD_NOTIFY_CMD` や parsable 出力などの通知・問い合わせ系の挙動

代表的なテストファイル:

- `cli_basic.rs`, `sbatch_options.rs`, `srun.rs`, `srun_options.rs`, `srun_modes.rs`, `salloc.rs`, `sinfo.rs`, `control.rs`, `query_squeue.rs`, `query_sacct.rs`
- `scheduling.rs`, `compound_scheduling.rs`, `dependency_variants.rs`, `array.rs`, `begin.rs`, `constraint.rs`, `resource_flags.rs`, `requeue.rs`, `timeout.rs`
- `srun_interactive.rs`, `srun_allocation.rs`, `cpu_bind.rs`, `output_files.rs`, `cancellation.rs`, `recovery.rs`, `update.rs`, `warning_signal.rs`, `notify.rs`

## 手動スモークテスト

簡単な手動確認を行う場合は、まず daemon を起動します。

```bash
cargo run -- daemon
```

その後、同じ `SLOTD_ROOT` を使う別シェルから小さなジョブを投げます。

```bash
cargo run -- sbatch --wrap 'echo hello'
```
