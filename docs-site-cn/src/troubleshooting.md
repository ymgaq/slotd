# 故障排查

## `Connection refused`

症状：

```text
error: io error: Connection refused (os error 111)
```

含义：

- client 找到了 socket 路径
- 但该路径上当前没有 daemon 在接受连接

检查项：

- daemon 是否真的在运行
- client 与 daemon 是否使用相同的 `SLOTD_ROOT`
- 是否残留了旧运行产生的过期 socket 文件

有用的检查命令：

```bash
ls -l "$SLOTD_ROOT/run/slotd.sock"
sinfo
```

## Runtime Root 不一致

如果 daemon 使用一个 runtime root，而 client 使用另一个，命令看起来会随机失败。

常见原因：

- daemon 由 `systemd --user` 启动
- client 从一个没有相同 `SLOTD_ROOT` 的 shell 中启动

最稳妥的修复方式：

- 使用 `scripts/install.sh` 安装
- 使用 `~/.local/bin` 下的 wrapper 命令

## 看不到 GPU 分区

如果 `sinfo` 只显示 `cpu`，请检查：

- `nvidia-smi` 是否在 daemon 环境中可用
- `SLOTD_GPU_PARTITIONS` 是否按预期配置
- 修改 GPU 配置后是否已经重启 daemon

手动检查：

```bash
nvidia-smi
sinfo
```

## 重装时出现 `Text file busy`

安装脚本现在会以原子方式替换二进制。如果仍然遇到问题：

- 停止或重启 daemon
- 再次运行安装脚本

命令：

```bash
systemctl --user restart slotd.service
./scripts/install.sh
```

## 作业一直处于 `PENDING`

可能原因：

- 资源不足
- dependency 未满足
- array 并发限制
- 尚未到达 delayed start time
- 作业处于 held 状态
- 已有 exclusive job 正在运行

排查方式：

```bash
squeue
scontrol show job <job_id>
```

## 作业被取消但最终状态是 `OUT_OF_MEMORY`

如果在终止过程中 cgroup 内存事件表明发生了 OOM，最终状态可能优先记录为 `OUT_OF_MEMORY`。

## 找不到输出文件

请检查：

- 作业的 working directory
- `-o/--output` 与 `-e/--error` 路径
- `%j`、`%A_%a` 等模式展开

排查命令：

```bash
scontrol show job <job_id>
```
