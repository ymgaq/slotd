# インストール

## 必要条件

- Linux または WSL
- `cargo` を含む Rust toolchain
- daemon を自動管理したい場合は `systemd --user`
- GPU の自動検出を使いたい場合は `nvidia-smi`

## 付属スクリプトによるインストール

リポジトリルートで実行します。

```bash
./scripts/install.sh
```

デフォルトでは次を行います。

- `slotd` を release ビルドする
- `slotd` を `~/.local/bin` にインストールする
- `sbatch` や `squeue` などのコマンド alias を作成する
- `~/.local/share/slotd` に runtime root を作成する
- `~/.config/slotd/slotd.env` を生成する
- `systemd --user` service をインストールして起動する

## インストーラのオプション

| オプション | 説明 | デフォルト |
| --- | --- | --- |
| `--repo-root PATH` | 別のリポジトリルートからビルドする | 現在の repo |
| `--profile NAME` | 使用する Cargo profile | `release` |
| `--install-bin-dir PATH` | バイナリと alias のインストール先 | `~/.local/bin` |
| `--runtime-root PATH` | `SLOTD_ROOT` として使う runtime root | `~/.local/share/slotd` |
| `--config-dir PATH` | 設定ディレクトリ | `~/.config/slotd` |
| `--systemd-user-dir PATH` | user unit ディレクトリ | `~/.config/systemd/user` |
| `--cpu-partitions VALUE` | `SLOTD_CPU_PARTITIONS` に書く値 | `cpu` |
| `--gpu-partitions VALUE` | `SLOTD_GPU_PARTITIONS` に書く値 | `gpu` |
| `--features VALUE` | `SLOTD_FEATURES` に書く値 | 未設定 |
| `--notify-cmd VALUE` | `SLOTD_NOTIFY_CMD` に書く値 | 未設定 |
| `--cgroup-base PATH` | `SLOTD_CGROUP_BASE` に書く値 | 未設定 |
| `--skip-build` | 既存のビルド成果物を再利用する | off |
| `--skip-systemd` | user service をインストール・起動しない | off |
| `--uninstall` | インストール済みの構成を削除する | off |
| `--purge-runtime` | uninstall 時に永続データも削除する | off |

例:

```bash
./scripts/install.sh \
  --features cpu,gpu \
  --notify-cmd 'notify-send "slotd" "$SLOTD_JOB_ID $SLOTD_JOB_STATE"'
```

## アンインストール

インストール済み構成を削除します。

```bash
./scripts/install.sh --uninstall
```

インストール済み構成と runtime state を両方削除します。

```bash
./scripts/install.sh --uninstall --purge-runtime
```

## 手動セットアップ

インストーラを使わない場合でも、直接ビルドして実行できます。

```bash
cargo build --release
SLOTD_ROOT="$HOME/.local/share/slotd" ./target/release/slotd daemon
```

別シェルでは同じ `SLOTD_ROOT` を使います。

```bash
SLOTD_ROOT="$HOME/.local/share/slotd" ./target/release/slotd sbatch --wrap 'echo hello'
```

## Runtime Files

デフォルトの runtime root は次です。

```text
~/.local/share/slotd
```

重要なファイルとディレクトリ:

- `run/slotd.sock`
- `lib/state.db`
- `lib/jobs/<job_id>/`

client と daemon は必ず同じ `SLOTD_ROOT` を使う必要があります。
