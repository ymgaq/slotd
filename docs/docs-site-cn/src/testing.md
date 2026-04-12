# 测试

`slotd` 主要通过 `tests/` 下的 Rust 集成测试进行验证。
每个测试都会创建一个独立的临时运行环境，启动专用 daemon，然后通过编译后的 `slotd` 二进制去驱动公开的 Slurm 风格命令。
因此测试不会污染你平时使用的 `SLOTD_ROOT`。

## 如何运行

运行完整测试套件：

```bash
cargo test
```

只运行某一个集成测试文件：

```bash
cargo test --test scheduling
```

只运行某一个具名测试用例：

```bash
cargo test dependency_job_waits_for_prerequisite_before_running --test scheduling
```

## 当前测试覆盖内容

当前测试套件重点覆盖对使用者有影响的行为：

- `sbatch`、`srun`、`salloc`、`sinfo`、`squeue`、`sacct`、`scontrol` 的核心命令流程
- 依赖、数组作业、延迟启动、constraint、resource flag、requeue 等调度规则
- `srun --pty`、`--label`、`--unbuffered` 以及 allocation / step 等交互式与前台执行路径
- cancellation、warning signal、update 处理、输出文件落盘等生命周期与恢复行为
- `SLOTD_NOTIFY_CMD` 与 parsable 查询输出等通知和报表相关路径

有代表性的测试文件包括：

- `cli_basic.rs`, `sbatch_options.rs`, `srun.rs`, `srun_options.rs`, `srun_modes.rs`, `salloc.rs`, `sinfo.rs`, `control.rs`, `query_output.rs`
- `scheduling.rs`, `compound_scheduling.rs`, `dependency_variants.rs`, `array.rs`, `begin.rs`, `constraint.rs`, `resource_flags.rs`, `requeue.rs`, `timeout.rs`
- `srun_interactive.rs`, `srun_allocation.rs`, `cpu_bind.rs`, `output_files.rs`, `cancellation.rs`, `recovery.rs`, `update.rs`, `warning_signal.rs`, `notify.rs`

## 手动冒烟测试

如果只想做一次快速手动验证，可以先启动 daemon：

```bash
cargo run -- daemon
```

然后在另一个使用同一 `SLOTD_ROOT` 的 shell 中提交一个小作业：

```bash
cargo run -- sbatch --wrap 'echo hello'
```
