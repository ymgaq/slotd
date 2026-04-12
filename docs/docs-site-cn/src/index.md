# slotd

`slotd` 是一个使用 Rust 实现的单节点、单用户、具备 Slurm 风格命令界面的调度器。

其他语言文档：
- [English](https://ymgaq.github.io/slotd/)
- [日本語版](https://ymgaq.github.io/slotd/ja/)

它面向的是一台工作站，而不是集群。目标是在保留常见 Slurm 命令名和主要选项的同时，显著简化运行模型。

- 一个本地 daemon
- 一个 SQLite 数据库
- 一个执行主机
- 一个本地用户工作流

你通过下面这些熟悉的命令来使用 `slotd`。

- `sbatch`
- `srun`
- `salloc`
- `squeue`
- `sacct`
- `scontrol`
- `scancel`
- `sinfo`

## 适合的场景

`slotd` 适合以下用途。

- 本地实验队列
- 长时间运行的 CPU / GPU 作业
- 单机批处理流水线
- 带资源预留的交互式工作
- 工作站上的轻量级 Slurm 风格接口

它不打算提供以下能力。

- 多节点调度
- 集群管理
- account、QoS 或 fairshare
- 跨主机 federation 或 reservation

## 主要特性

- 作为单一 Rust 二进制程序构建
- 使用 daemon 与 Unix domain socket
- 使用 SQLite 持久化状态
- 调度 CPU、内存和 GPU 预留
- 支持批处理作业、数组作业、交互执行、allocation 与 step
- 支持延迟启动、单次 requeue、依赖关系以及本地 feature constraint

## 文档目录

- [安装](installation.md)
- [快速开始](quickstart.md)
- [运行模型](runtime-model.md)
- [`sbatch` 批处理作业](sbatch.md)
- [`srun` 交互执行](srun.md)
- [`salloc` 资源分配](salloc.md)
- [队列与记账信息](queue-and-accounting.md)
- [作业控制](job-control.md)
- [节点与分区视图](sinfo.md)
- [测试](testing.md)
- [示例](examples.md)
- [故障排查](troubleshooting.md)
