use std::path::Path;

pub enum LaunchCommand<'a> {
    Script(&'a Path),
    Command(&'a [String]),
}

pub fn build_multitask_launcher(
    command: LaunchCommand<'_>,
    requested_tasks: u32,
    label_output: bool,
) -> String {
    let requested_tasks = requested_tasks.max(1);
    let command_text = match command {
        LaunchCommand::Script(path) => {
            format!("/bin/bash {}", shell_quote(&path.to_string_lossy()))
        }
        LaunchCommand::Command(args) => shell_join(args),
    };

    let mut script = String::from(
        "#!/usr/bin/env bash\n\
set +e\n\
pids=()\n",
    );
    script.push_str(&format!("slotd_task_count={requested_tasks}\n"));
    script
        .push_str("for ((slotd_task_id=0; slotd_task_id<slotd_task_count; slotd_task_id++)); do\n");
    script.push_str("  (\n");
    script.push_str("    export SLURM_PROCID=\"$slotd_task_id\"\n");
    script.push_str("    export SLURM_LOCALID=\"$slotd_task_id\"\n");
    script.push_str("    export SLURM_NODEID=0\n");
    script.push_str("    export SLURM_TASK_PID=\"$$\"\n");
    script.push_str("    if [ \"$slotd_task_id\" -ne 0 ]; then\n");
    script.push_str("      exec </dev/null\n");
    script.push_str("    fi\n");
    if label_output {
        script.push_str("    exec > >(sed -u \"s/^/${slotd_task_id}: /\")\n");
        script.push_str("    exec 2> >(sed -u \"s/^/${slotd_task_id}: /\" >&2)\n");
    }
    script.push_str(&format!("    exec {command_text}\n"));
    script.push_str("  ) &\n");
    script.push_str("  pids+=(\"$!\")\n");
    script.push_str("done\n");
    script.push_str("slotd_status=0\n");
    script.push_str("for pid in \"${pids[@]}\"; do\n");
    script.push_str("  wait \"$pid\"\n");
    script.push_str("  code=$?\n");
    script.push_str("  if [ \"$code\" -ne 0 ] && [ \"$slotd_status\" -eq 0 ]; then\n");
    script.push_str("    slotd_status=\"$code\"\n");
    script.push_str("  fi\n");
    script.push_str("done\n");
    script.push_str("exit \"$slotd_status\"\n");
    script
}

pub fn shell_join(args: &[String]) -> String {
    args.iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }

    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '.' | ':'))
    {
        return value.to_string();
    }

    format!("'{}'", value.replace('\'', "'\"'\"'"))
}
