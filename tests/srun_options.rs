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
    let pwd_path = runtime.root_dir().join("srun-pwd.txt");
    let env_path = runtime.root_dir().join("srun-env.txt");

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
            "pwd > \"{}\"; printf '%s|%s|%s|%s|%s|%s' \"$SLURM_JOB_ID\" \"$SLURM_JOB_NAME\" \"$SLURM_JOB_PARTITION\" \"$SLURM_NTASKS\" \"$SLURM_CPUS_PER_TASK\" \"$SLURM_STEP_ID\" > \"{}\"",
            pwd_path.display(),
            env_path.display()
        ),
    ]);
    assert!(
        output.status.success(),
        "srun failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let pwd = fs::read_to_string(&pwd_path).expect("read srun pwd");
    assert_eq!(pwd.trim(), workdir.display().to_string());

    let env = fs::read_to_string(&env_path).expect("read srun env");
    let mut parts = env.trim().split('|');
    let job_id = parts
        .next()
        .expect("job id")
        .parse::<i64>()
        .expect("parse job id");
    assert_eq!(parts.next(), Some("bash"));
    assert_eq!(parts.next(), Some("gpu"));
    let _ = parts.next().expect("ntasks field");
    let _ = parts.next().expect("cpus-per-task field");
    assert_eq!(parts.next(), Some("0"));

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
