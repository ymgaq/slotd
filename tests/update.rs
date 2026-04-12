mod helpers;

use std::time::Duration;

use helpers::TestRuntime;

#[test]
fn scontrol_update_changes_pending_job_fields() {
    let runtime = TestRuntime::new();

    let blocker_job_id = runtime
        .run_checked(&["sbatch", "--parsable", "--wrap", "sleep 2"])
        .parse::<i64>()
        .expect("parse blocker job id");
    let dependency = format!("afterok:{blocker_job_id}");
    let job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "--dependency",
            &dependency,
            "--wrap",
            "true",
        ])
        .parse::<i64>()
        .expect("parse job id");

    let pending_state = runtime.wait_for_job_state(job_id, "PENDING", Duration::from_secs(2));
    assert_eq!(pending_state, "PENDING");

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
    assert!(details.contains("JobName=updated-name"), "details:\n{details}");
    assert!(details.contains("TimeLimit=00:00:05"), "details:\n{details}");
    assert!(details.contains("State=PENDING"), "details:\n{details}");

    let blocker_state =
        runtime.wait_for_job_state(blocker_job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(blocker_state, "COMPLETED");

    let final_state = runtime.wait_for_job_state(job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(final_state, "COMPLETED");
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
