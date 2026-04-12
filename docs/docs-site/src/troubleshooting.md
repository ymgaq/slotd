# Troubleshooting

## `Connection refused`

Symptom:

```text
error: io error: Connection refused (os error 111)
```

Meaning:

- the client found a socket path
- but no daemon is currently accepting connections there

Check:

- the daemon is actually running
- the client and the daemon use the same `SLOTD_ROOT`
- you do not have a stale socket file from an older run

Useful checks:

```bash
ls -l "$SLOTD_ROOT/run/slotd.sock"
sinfo
```

## Wrong Runtime Root

If the daemon uses one runtime root and the client uses another, commands appear to fail randomly.

Common cause:

- daemon started by `systemd --user`
- client launched from a shell without the same `SLOTD_ROOT`

Best fix:

- install with `scripts/install.sh`
- use the wrapper commands from `~/.local/bin`

## No GPU Partition Appears

If `sinfo` only shows `cpu`, check:

- `nvidia-smi` works in the daemon environment
- `SLOTD_GPU_PARTITIONS` is configured as expected
- the daemon has been restarted after GPU configuration changes

Manual check:

```bash
nvidia-smi
sinfo
```

## `Text file busy` During Reinstall

The installer now replaces binaries atomically. If you still hit issues:

- stop the daemon
- run the installer again

Commands:

```bash
systemctl --user restart slotd.service
./scripts/install.sh
```

## Jobs Stay Pending

Possible causes:

- insufficient resources
- dependency not satisfied
- array concurrency limit
- delayed start time not reached
- job is held
- an exclusive job is already running

Inspect with:

```bash
squeue
scontrol show job <job_id>
```

## cgroup Setup Fails

If `SLOTD_CGROUP_BASE` is set but does not point at a writable cgroup v2
subtree, job launch fails with an explicit cgroup error.

Check:

- the path exists in the daemon environment
- the path is a cgroup v2 subtree, not a regular directory or file
- the daemon user can create per-job subdirectories and write control files there

## A Job Was Cancelled but Ended as `OUT_OF_MEMORY`

This can happen if cgroup memory events indicate an OOM during termination. In that case the final state prefers `OUT_OF_MEMORY`.

## Output Files Are Missing

Check:

- the job's working directory
- the `-o/--output` and `-e/--error` paths
- pattern expansion such as `%j` or `%A_%a`

Inspect with:

```bash
scontrol show job <job_id>
```
