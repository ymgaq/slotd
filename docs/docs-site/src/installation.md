# Installation

## Requirements

- Linux or WSL
- Rust toolchain with `cargo`
- `systemd --user` if you want automatic daemon management
- `nvidia-smi` if you want automatic GPU detection

## Clone the Repository

```bash
git clone https://github.com/ymgaq/slotd.git
cd slotd
```

## Install with the Provided Script

From the repository root:

```bash
./scripts/install.sh
```

By default this will:

- build `slotd` in release mode
- install `slotd` under `~/.local/bin`
- create command aliases such as `sbatch` and `squeue`
- create a runtime root under `~/.local/share/slotd`
- write `~/.config/slotd/slotd.env`
- install and start a `systemd --user` service

## Installer Options

| Option | Description | Default |
| --- | --- | --- |
| `--repo-root PATH` | Build from a different repository root | current repo |
| `--profile NAME` | Cargo profile to build | `release` |
| `--install-bin-dir PATH` | Install binary and alias directory | `~/.local/bin` |
| `--runtime-root PATH` | Runtime root used as `SLOTD_ROOT` | `~/.local/share/slotd` |
| `--config-dir PATH` | Configuration directory | `~/.config/slotd` |
| `--systemd-user-dir PATH` | User unit directory | `~/.config/systemd/user` |
| `--cpu-partitions VALUE` | Value for `SLOTD_CPU_PARTITIONS` | `cpu` |
| `--gpu-partitions VALUE` | Value for `SLOTD_GPU_PARTITIONS` | `gpu` |
| `--features VALUE` | Value for `SLOTD_FEATURES` | unset |
| `--notify-cmd VALUE` | Value for `SLOTD_NOTIFY_CMD` | unset |
| `--cgroup-base PATH` | Value for `SLOTD_CGROUP_BASE` | unset |
| `--skip-build` | Reuse an existing build output | off |
| `--skip-systemd` | Do not install or start a user service | off |
| `--uninstall` | Remove the installed setup | off |
| `--purge-runtime` | Remove persisted state during uninstall | off |

Example:

```bash
./scripts/install.sh \
  --features cpu,gpu \
  --notify-cmd 'notify-send "slotd" "$SLOTD_JOB_ID $SLOTD_JOB_STATE"'
```

If you set `--cgroup-base`, use a writable cgroup v2 subtree. Leaving it unset
keeps CPU and memory as reservation-only scheduling values.

## Uninstall

Remove the installation:

```bash
./scripts/install.sh --uninstall
```

Remove installation and runtime state:

```bash
./scripts/install.sh --uninstall --purge-runtime
```

## Manual Setup

If you do not want to use the installer, you can still build and run `slotd` directly:

```bash
cargo build --release
SLOTD_ROOT="$HOME/.local/share/slotd" ./target/release/slotd daemon
```

Then use the same `SLOTD_ROOT` in another shell:

```bash
SLOTD_ROOT="$HOME/.local/share/slotd" ./target/release/slotd sbatch --wrap 'echo hello'
```

## Runtime Files

The default runtime root is:

```text
~/.local/share/slotd
```

Important files and directories:

- `run/slotd.sock`
- `lib/state.db`
- `lib/jobs/<job_id>/`

The client and the daemon must use the same `SLOTD_ROOT`.
