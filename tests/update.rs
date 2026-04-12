mod helpers;

use std::time::Duration;

use helpers::TestRuntime;

fn submit_pending_job(runtime: &TestRuntime, blocker_partition: &str, args: &[&str]) -> (i64, i64) {
    let blocker_job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "--partition",
            blocker_partition,
            "--wrap",
            "sleep 2",
        ])
        .parse::<i64>()
        .expect("parse blocker job id");
    let dependency = format!("afterok:{blocker_job_id}");
    let mut command = vec!["sbatch", "--parsable", "--dependency", &dependency];
    command.extend_from_slice(args);
    let job_id = runtime
        .run_checked(&command)
        .parse::<i64>()
        .expect("parse job id");

    let pending_state = runtime.wait_for_job_state(job_id, "PENDING", Duration::from_secs(2));
    assert_eq!(pending_state, "PENDING");

    (blocker_job_id, job_id)
}

#[test]
fn scontrol_update_changes_pending_job_fields() {
    let runtime = TestRuntime::new();

    let (blocker_job_id, job_id) = submit_pending_job(&runtime, "cpu", &["--wrap", "true"]);

    let output = runtime.run_checked(&[
        "scontrol",
        "update",
        "job",
        &job_id.to_string(),
        "JobName=updated-name",
        "TimeLimit=00:00:05",
    ]);
    assert!(output.is_empty(), "stdout:\n{output}");

    let details = runtime.scontrol_show_job(job_id);
    assert!(
        details.contains("JobName=updated-name"),
        "details:\n{details}"
    );
    assert!(
        details.contains("TimeLimit=00:00:05"),
        "details:\n{details}"
    );
    assert!(details.contains("State=PENDING"), "details:\n{details}");

    let blocker_state =
        runtime.wait_for_job_state(blocker_job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(blocker_state, "COMPLETED");

    let final_state = runtime.wait_for_job_state(job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(final_state, "COMPLETED");
}

#[test]
fn scontrol_update_changes_pending_partition_and_queue_view() {
    let runtime = TestRuntime::with_env(&[
        ("SLOTD_GPU_COUNT", "1"),
        ("SLOTD_CPU_PARTITIONS", "cpu"),
        ("SLOTD_GPU_PARTITIONS", "gpu"),
    ]);

    let (blocker_job_id, job_id) =
        submit_pending_job(&runtime, "cpu", &["--partition", "cpu", "--wrap", "true"]);

    let output = runtime.run_checked(&[
        "scontrol",
        "update",
        "job",
        &job_id.to_string(),
        "Partition=gpu",
    ]);
    assert!(output.is_empty(), "stdout:\n{output}");

    let details = runtime.scontrol_show_job(job_id);
    assert!(details.contains("Partition=gpu"), "details:\n{details}");

    let queue = runtime.run_checked(&[
        "squeue",
        "-j",
        &job_id.to_string(),
        "-o",
        "%P",
        "--noheader",
    ]);
    assert_eq!(queue.trim(), "gpu", "squeue:\n{queue}");

    let blocker_state =
        runtime.wait_for_job_state(blocker_job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(blocker_state, "COMPLETED");
    let final_state = runtime.wait_for_job_state(job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(final_state, "COMPLETED");
}

#[test]
fn scontrol_update_priority_changes_pending_schedule_order() {
    let runtime = TestRuntime::new();

    let blocker_job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "--partition",
            "cpu",
            "--exclusive",
            "--wrap",
            "sleep 2",
        ])
        .parse::<i64>()
        .expect("parse blocker job id");
    let blocker_state =
        runtime.wait_for_job_state(blocker_job_id, "RUNNING", Duration::from_secs(5));
    assert_eq!(blocker_state, "RUNNING");

    let first_job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "--partition",
            "cpu",
            "--exclusive",
            "--wrap",
            "sleep 1",
        ])
        .parse::<i64>()
        .expect("parse first job id");
    let second_job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "--partition",
            "cpu",
            "--exclusive",
            "--wrap",
            "sleep 1",
        ])
        .parse::<i64>()
        .expect("parse second job id");

    assert_eq!(
        runtime.wait_for_job_state(first_job_id, "PENDING", Duration::from_secs(2)),
        "PENDING"
    );
    assert_eq!(
        runtime.wait_for_job_state(second_job_id, "PENDING", Duration::from_secs(2)),
        "PENDING"
    );

    let output = runtime.run_checked(&[
        "scontrol",
        "update",
        "job",
        &second_job_id.to_string(),
        "Priority=100",
    ]);
    assert!(output.is_empty(), "stdout:\n{output}");

    let blocker_final =
        runtime.wait_for_job_state(blocker_job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(blocker_final, "COMPLETED");

    let running_job = runtime.wait_for_job_state_in(
        second_job_id,
        &["RUNNING", "COMPLETED"],
        Duration::from_secs(5),
    );
    assert!(matches!(running_job.as_str(), "RUNNING" | "COMPLETED"));
    runtime.assert_job_state_stable(first_job_id, "PENDING", Duration::from_millis(500));

    let second_final =
        runtime.wait_for_job_state(second_job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(second_final, "COMPLETED");
    let first_final =
        runtime.wait_for_job_state(first_job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(first_final, "COMPLETED");
}

#[test]
fn scontrol_update_rejects_partition_change_when_constraint_no_longer_matches() {
    let runtime = TestRuntime::with_env(&[
        ("SLOTD_GPU_COUNT", "1"),
        ("SLOTD_CPU_PARTITIONS", "alpha"),
        ("SLOTD_GPU_PARTITIONS", "beta"),
    ]);

    let (_blocker_job_id, job_id) = submit_pending_job(
        &runtime,
        "alpha",
        &[
            "--partition",
            "alpha",
            "--constraint",
            "alpha",
            "--wrap",
            "true",
        ],
    );

    let output = runtime.run_output(&[
        "scontrol",
        "update",
        "job",
        &job_id.to_string(),
        "Partition=beta",
    ]);
    assert!(!output.status.success(), "unexpected success");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("constraint \"alpha\" does not match local features for partition beta"),
        "stderr:\n{stderr}"
    );
}

#[test]
fn scontrol_update_rejects_job_name_change_after_completion() {
    let runtime = TestRuntime::new();

    let job_id = runtime
        .run_checked(&["sbatch", "--parsable", "--wrap", "true"])
        .parse::<i64>()
        .expect("parse job id");

    let final_state = runtime.wait_for_job_state(job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(final_state, "COMPLETED");

    let output = runtime.run_output(&[
        "scontrol",
        "update",
        "job",
        &job_id.to_string(),
        "JobName=too-late",
    ]);
    assert!(
        !output.status.success(),
        "scontrol update unexpectedly succeeded\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("job name can only be updated while pending"),
        "stderr:\n{stderr}"
    );
}

#[test]
fn scontrol_update_rejects_priority_change_after_completion() {
    let runtime = TestRuntime::new();

    let job_id = runtime
        .run_checked(&["sbatch", "--parsable", "--wrap", "true"])
        .parse::<i64>()
        .expect("parse job id");

    let final_state = runtime.wait_for_job_state(job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(final_state, "COMPLETED");

    let output = runtime.run_output(&[
        "scontrol",
        "update",
        "job",
        &job_id.to_string(),
        "Priority=999",
    ]);
    assert!(!output.status.success(), "unexpected success");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("priority can only be updated while pending"),
        "stderr:\n{stderr}"
    );
}

#[test]
fn scontrol_update_rejects_partition_change_after_completion() {
    let runtime = TestRuntime::new();

    let job_id = runtime
        .run_checked(&["sbatch", "--parsable", "--wrap", "true"])
        .parse::<i64>()
        .expect("parse job id");

    let final_state = runtime.wait_for_job_state(job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(final_state, "COMPLETED");

    let output = runtime.run_output(&[
        "scontrol",
        "update",
        "job",
        &job_id.to_string(),
        "Partition=cpu",
    ]);
    assert!(!output.status.success(), "unexpected success");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("partition can only be updated while pending"),
        "stderr:\n{stderr}"
    );
}

#[test]
fn scontrol_update_rejects_time_limit_change_after_completion() {
    let runtime = TestRuntime::new();

    let job_id = runtime
        .run_checked(&["sbatch", "--parsable", "--wrap", "true"])
        .parse::<i64>()
        .expect("parse job id");

    let final_state = runtime.wait_for_job_state(job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(final_state, "COMPLETED");

    let output = runtime.run_output(&[
        "scontrol",
        "update",
        "job",
        &job_id.to_string(),
        "TimeLimit=00:00:30",
    ]);
    assert!(!output.status.success(), "unexpected success");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("time limit cannot be updated after the job has finished"),
        "stderr:\n{stderr}"
    );
}
