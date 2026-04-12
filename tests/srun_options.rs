mod helpers;

use std::fs;
use std::time::Duration;

use helpers::TestRuntime;

#[test]
fn srun_resource_flags_are_visible_at_runtime() {
    let runtime = TestRuntime::with_env(&[
        ("SLOTD_GPU_COUNT", "1"),
        ("SLOTD_CPU_PARTITIONS", "cpu"),
        ("SLOTD_GPU_PARTITIONS", "gpu"),
    ]);
    let workdir = runtime.root_dir().join("srun-workdir");
    fs::create_dir_all(&workdir).expect("create srun workdir");
    let task_dir = runtime.root_dir().join("srun-task-env");
    fs::create_dir_all(&task_dir).expect("create srun task dir");

    let output = runtime.run_output(&[
        "srun",
        "-J",
        "srun-check",
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
        "--",
        "bash",
        "-lc",
        &format!(
            "pwd > \"{task_dir}/pwd-$SLURM_PROCID.txt\"; printf '%s|%s|%s|%s|%s|%s|%s' \"$SLURM_JOB_ID\" \"$SLURM_JOB_NAME\" \"$SLURM_JOB_PARTITION\" \"$SLURM_NTASKS\" \"$SLURM_CPUS_PER_TASK\" \"$SLURM_STEP_ID\" \"$SLURM_PROCID\" > \"{task_dir}/env-$SLURM_PROCID.txt\"",
            task_dir = task_dir.display(),
        ),
    ]);
    assert!(
        output.status.success(),
        "srun failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let mut job_id = None;
    for procid in 0..2 {
        let pwd =
            fs::read_to_string(task_dir.join(format!("pwd-{procid}.txt"))).expect("read srun pwd");
        assert_eq!(pwd.trim(), workdir.display().to_string());

        let env =
            fs::read_to_string(task_dir.join(format!("env-{procid}.txt"))).expect("read srun env");
        let mut parts = env.trim().split('|');
        let current_job_id = parts
            .next()
            .expect("job id")
            .parse::<i64>()
            .expect("parse job id");
        assert_eq!(parts.next(), Some("bash"));
        assert_eq!(parts.next(), Some("gpu"));
        assert_eq!(parts.next(), Some("2"));
        assert_eq!(parts.next(), Some("1"));
        assert_eq!(parts.next(), Some("0"));
        assert_eq!(parts.next(), Some(&procid.to_string()[..]));
        if let Some(job_id) = job_id {
            assert_eq!(current_job_id, job_id);
        } else {
            job_id = Some(current_job_id);
        }
    }
    let job_id = job_id.expect("srun job id");

    let final_state = runtime.wait_for_job_state(job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(final_state, "COMPLETED");

    let details = runtime.scontrol_show_job(job_id);
    assert!(
        details.contains("JobName=srun-check"),
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

#[test]
fn srun_ntasks_failure_returns_non_zero_and_marks_job_failed() {
    let runtime = TestRuntime::new();
    let task_dir = runtime.root_dir().join("srun-fail-task-env");
    fs::create_dir_all(&task_dir).expect("create srun fail task dir");

    let output = runtime.run_output(&[
        "srun",
        "-p",
        "cpu",
        "-n",
        "3",
        "--",
        "bash",
        "-lc",
        &format!(
            "printf '%s|%s' \"$SLURM_JOB_ID\" \"$SLURM_PROCID\" > \"{task_dir}/env-$SLURM_PROCID.txt\"; if [ \"$SLURM_PROCID\" -eq 1 ]; then exit 7; fi",
            task_dir = task_dir.display(),
        ),
    ]);
    assert_eq!(
        output.status.code(),
        Some(7),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let env = fs::read_to_string(task_dir.join("env-0.txt")).expect("read srun fail env");
    let job_id = env
        .trim()
        .split('|')
        .next()
        .expect("job id")
        .parse::<i64>()
        .expect("parse job id");

    let final_state = runtime.wait_for_job_state(job_id, "FAILED", Duration::from_secs(10));
    assert_eq!(final_state, "FAILED");
}
