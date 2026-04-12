mod helpers;

use std::time::Duration;

use helpers::TestRuntime;

#[test]
fn sbatch_resource_flags_appear_in_job_details() {
    let runtime = TestRuntime::new();

    let blocker_job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "--partition",
            "cpu",
            "--wrap",
            "sleep 2",
        ])
        .parse::<i64>()
        .expect("parse blocker job id");
    let dependency = format!("afterok:{blocker_job_id}");
    let job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "-J",
            "resource-check",
            "-p",
            "gpu",
            "-c",
            "1",
            "-n",
            "1",
            "--mem",
            "1G",
            "-G",
            "1",
            "--exclusive",
            "--dependency",
            &dependency,
            "--wrap",
            "true",
        ])
        .parse::<i64>()
        .expect("parse resource job id");

    let pending_state = runtime.wait_for_job_state(job_id, "PENDING", Duration::from_secs(2));
    assert_eq!(pending_state, "PENDING");

    let details = runtime.scontrol_show_job(job_id);
    assert!(
        details.contains("JobName=resource-check"),
        "details:\n{details}"
    );
    assert!(details.contains("Partition=gpu"), "details:\n{details}");
    assert!(details.contains("NumTasks=1"), "details:\n{details}");
    assert!(details.contains("CPUs/Task=1"), "details:\n{details}");
    assert!(details.contains("ReqMem=1024MB"), "details:\n{details}");
    assert!(details.contains("ReqGRES=gpu:1"), "details:\n{details}");
    assert!(details.contains("Exclusive=Yes"), "details:\n{details}");
}

#[test]
fn exclusive_job_blocks_other_top_level_jobs_until_completion() {
    let runtime = TestRuntime::new();

    let exclusive_job_id = runtime
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
        .expect("parse exclusive job id");
    let running_state =
        runtime.wait_for_job_state(exclusive_job_id, "RUNNING", Duration::from_secs(5));
    assert_eq!(running_state, "RUNNING");

    let blocked_job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "--partition",
            "cpu",
            "--wrap",
            "true",
        ])
        .parse::<i64>()
        .expect("parse blocked job id");

    let pending_state =
        runtime.wait_for_job_state(blocked_job_id, "PENDING", Duration::from_secs(2));
    assert_eq!(pending_state, "PENDING");
    runtime.assert_job_state_stable(blocked_job_id, "PENDING", Duration::from_millis(500));

    let exclusive_final =
        runtime.wait_for_job_state(exclusive_job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(exclusive_final, "COMPLETED");
    let blocked_final =
        runtime.wait_for_job_state(blocked_job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(blocked_final, "COMPLETED");
}
