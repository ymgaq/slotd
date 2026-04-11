# 安装

## 运行要求

- Linux 或 WSL
- 带 `cargo` 的 Rust toolchain
- 如果希望自动管理 daemon，则需要 `systemd --user`
- 如果希望自动检测 GPU，则需要 `nvidia-smi`

## 使用附带脚本安装

在仓库根目录运行：

```bash
./scripts/install.sh
```

默认会执行以下操作：

- 以 release 模式构建 `slotd`
- 将 `slotd` 安装到 `~/.local/bin`
- 创建 `sbatch`、`squeue` 等命令别名
- 在 `~/.local/share/slotd` 下创建 runtime root
- 写入 `~/.config/slotd/slotd.env`
- 安装并启动 `systemd --user` 服务

## 安装脚本选项

| 选项 | 说明 | 默认值 |
| --- | --- | --- |
| `--repo-root PATH` | 从其他仓库根目录构建 | 当前仓库 |
| `--profile NAME` | 使用的 Cargo profile | `release` |
| `--install-bin-dir PATH` | 二进制和别名安装目录 | `~/.local/bin` |
| `--runtime-root PATH` | 作为 `SLOTD_ROOT` 的 runtime root | `~/.local/share/slotd` |
| `--config-dir PATH` | 配置目录 | `~/.config/slotd` |
| `--systemd-user-dir PATH` | user unit 目录 | `~/.config/systemd/user` |
| `--cpu-partitions VALUE` | 写入 `SLOTD_CPU_PARTITIONS` 的值 | `cpu` |
| `--gpu-partitions VALUE` | 写入 `SLOTD_GPU_PARTITIONS` 的值 | `gpu` |
| `--features VALUE` | 写入 `SLOTD_FEATURES` 的值 | 未设置 |
| `--notify-cmd VALUE` | 写入 `SLOTD_NOTIFY_CMD` 的值 | 未设置 |
| `--cgroup-base PATH` | 写入 `SLOTD_CGROUP_BASE` 的值 | 未设置 |
| `--skip-build` | 复用已有构建产物 | off |
| `--skip-systemd` | 不安装或启动 user service | off |
| `--uninstall` | 删除已安装内容 | off |
| `--purge-runtime` | 卸载时删除持久化状态 | off |

示例：

```bash
./scripts/install.sh \
  --features cpu,gpu \
  --notify-cmd 'notify-send "slotd" "$SLOTD_JOB_ID $SLOTD_JOB_STATE"'
```

## 卸载

删除安装内容：

```bash
./scripts/install.sh --uninstall
```

删除安装内容和 runtime 状态：

```bash
./scripts/install.sh --uninstall --purge-runtime
```

## 手动安装

如果不想使用安装脚本，也可以直接构建并运行 `slotd`：

```bash
cargo build --release
SLOTD_ROOT="$HOME/.local/share/slotd" ./target/release/slotd daemon
```

然后在另一个 shell 中使用相同的 `SLOTD_ROOT`：

```bash
SLOTD_ROOT="$HOME/.local/share/slotd" ./target/release/slotd sbatch --wrap 'echo hello'
```

## Runtime 文件

默认的 runtime root 为：

```text
~/.local/share/slotd
```

重要文件和目录：

- `run/slotd.sock`
- `lib/state.db`
- `lib/jobs/<job_id>/`

client 与 daemon 必须使用相同的 `SLOTD_ROOT`。
