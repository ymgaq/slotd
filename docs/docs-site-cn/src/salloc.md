# `salloc` 资源分配

## 形式

```bash
salloc [options] [command...]
```

## `salloc` 的作用

`salloc` 会创建一个仅 allocation 的顶层 job，等待其变为可运行，然后在该 allocation 内启动一个前台命令。

如果没有提供命令，则启动你的 shell。

典型输出：

```text
Granted job allocation 4
```

## 主要选项

| 选项 | 含义 |
| --- | --- |
| `-J`, `--job-name <name>` | 设置 allocation 名称 |
| `-p`, `--partition <partition>` | 选择分区 |
| `-c`, `--cpus-per-task <n>` | 每个 task 的 CPU 数 |
| `-n`, `--ntasks <n>` | task 数量 |
| `--mem <size>` | 请求内存 |
| `-t`, `--time <time>` | 时间限制 |
| `-G`, `--gpus <n>` | 请求 GPU slot 数 |
| `-D`, `--chdir <path>` | 工作目录 |
| `--constraint <feature>` | 要求匹配的本地 feature |
| `--immediate` | allocation 无法立即启动时直接失败 |

## 示例

```bash
salloc -p gpu -c 4 --mem 8G -G 1 -t 00:30:00
```

预期结果：

- 创建一个 allocation 记录
- 命令会等待 allocation 进入运行状态
- 在 allocation 内启动你的 shell
- 之后的 `srun` 会成为该 allocation 下的 step
