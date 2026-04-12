mod helpers;

use std::fs;
use std::time::Duration;

use helpers::TestRuntime;

#[test]
fn salloc_without_command_launches_shell_from_environment() {
    let shell_script = std::env::temp_dir().join(format!(
        "slotd-salloc-shell-{}-{}.sh",
        std::process::id(),
        std::thread::current().name().unwrap_or("main")
    ));
    let marker_path = std::env::temp_dir().join(format!(
        "slotd-salloc-shell-marker-{}-{}.txt",
        std::process::id(),
        std::thread::current().name().unwrap_or("main")
    ));
    fs::write(
        &shell_script,
        format!(
            "#!/usr/bin/env bash\nprintf 'shell:%s' \"$SLURM_JOB_ID\" > \"{}\"\n",
            marker_path.display()
        ),
    )
    .expect("write shell script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&shell_script)
            .expect("shell metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&shell_script, perms).expect("set shell permissions");
    }

    let runtime = TestRuntime::with_env(&[("SHELL", shell_script.to_str().expect("shell path"))]);

    let output = runtime.run_output(&["salloc", "-p", "cpu"]);
    assert!(
        output.status.success(),
        "salloc failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let allocation_job_id = stdout
        .lines()
        .find_map(|line| line.strip_prefix("Granted job allocation "))
        .expect("allocation id line")
        .parse::<i64>()
        .expect("parse allocation job id");

    let marker = fs::read_to_string(&marker_path).expect("read shell marker");
    assert_eq!(marker.trim(), format!("shell:{allocation_job_id}"));

    let final_state =
        runtime.wait_for_job_state(allocation_job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(final_state, "COMPLETED");

    let _ = fs::remove_file(&shell_script);
    let _ = fs::remove_file(&marker_path);
}

#[test]
fn salloc_resource_flags_are_visible_at_runtime() {
    let runtime = TestRuntime::with_env(&[
        ("SLOTD_GPU_COUNT", "1"),
        ("SLOTD_CPU_PARTITIONS", "cpu"),
        ("SLOTD_GPU_PARTITIONS", "gpu"),
        ("SLOTD_FEATURES", "gpu,accel"),
    ]);
    let workdir = runtime.root_dir().join("salloc-workdir");
    fs::create_dir_all(&workdir).expect("create salloc workdir");
    let task_dir = runtime.root_dir().join("salloc-task-env");
    fs::create_dir_all(&task_dir).expect("create salloc task dir");

    let output = runtime.run_output(&[
        "salloc",
        "-J",
        "alloc-check",
        "-p",
        "gpu",
        "-n",
        "2",
        "-c",
        "1",
        "--mem",
        "1G",
        "-G",
        "1",
        "-t",
        "00:00:05",
        "-D",
        workdir.to_str().expect("workdir"),
        "--constraint",
        "accel",
        "bash",
        "-lc",
        &format!(
            "pwd > \"{task_dir}/pwd-$SLURM_PROCID.txt\"; printf '%s|%s|%s|%s|%s|%s' \"$SLURM_JOB_ID\" \"$SLURM_JOB_NAME\" \"$SLURM_JOB_PARTITION\" \"$SLURM_NTASKS\" \"$SLURM_CPUS_PER_TASK\" \"$SLURM_PROCID\" > \"{task_dir}/env-$SLURM_PROCID.txt\"",
            task_dir = task_dir.display(),
        ),
    ]);
    assert!(
        output.status.success(),
        "salloc failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let allocation_job_id = stdout
        .lines()
        .find_map(|line| line.strip_prefix("Granted job allocation "))
        .expect("allocation id line")
        .parse::<i64>()
        .expect("parse allocation job id");

    for procid in 0..2 {
        let pwd = fs::read_to_string(task_dir.join(format!("pwd-{procid}.txt")))
            .expect("read salloc pwd");
        assert_eq!(pwd.trim(), workdir.display().to_string());

        let env = fs::read_to_string(task_dir.join(format!("env-{procid}.txt")))
            .expect("read salloc env");
        assert_eq!(
            env.trim(),
            format!("{allocation_job_id}|alloc-check|gpu|2|1|{procid}")
        );
    }

    let final_state =
        runtime.wait_for_job_state(allocation_job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(final_state, "COMPLETED");

    let details = runtime.scontrol_show_job(allocation_job_id);
    assert!(
        details.contains("JobName=alloc-check"),
        "details:\n{details}"
    );
    assert!(details.contains("Partition=gpu"), "details:\n{details}");
    assert!(details.contains("NumTasks=2"), "details:\n{details}");
    assert!(details.contains("CPUs/Task=1"), "details:\n{details}");
    assert!(details.contains("ReqMem=1024MB"), "details:\n{details}");
    assert!(details.contains("ReqGRES=gpu:1"), "details:\n{details}");
    assert!(
        details.contains("TimeLimit=00:00:05"),
        "details:\n{details}"
    );
    assert!(
        details.contains(&format!("WorkDir={}", workdir.display())),
        "details:\n{details}"
    );
}
