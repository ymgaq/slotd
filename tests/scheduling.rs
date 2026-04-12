mod helpers;

use std::time::Duration;

use helpers::TestRuntime;

#[test]
fn dependency_job_waits_for_prerequisite_before_running() {
    let runtime = TestRuntime::new();

    let blocker_job_id = runtime
        .run_checked(&["sbatch", "--parsable", "--wrap", "sleep 2"])
        .parse::<i64>()
        .expect("parse blocker job id");
    let dependency = format!("afterok:{blocker_job_id}");
    let dependent_job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "--dependency",
            &dependency,
            "--wrap",
            "true",
        ])
        .parse::<i64>()
        .expect("parse dependent job id");

    let state = runtime.wait_for_job_state(dependent_job_id, "PENDING", Duration::from_secs(2));
    assert_eq!(state, "PENDING");
    runtime.assert_job_state_stable(dependent_job_id, "PENDING", Duration::from_millis(500));

    let blocker_state =
        runtime.wait_for_job_state(blocker_job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(blocker_state, "COMPLETED");

    let dependent_state =
        runtime.wait_for_job_state(dependent_job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(dependent_state, "COMPLETED");
}

#[test]
fn scontrol_show_job_reports_core_fields_for_pending_job() {
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
            "--job-name",
            "inspect-me",
            "--partition",
            "cpu",
            "--dependency",
            &dependency,
            "--wrap",
            "true",
        ])
        .parse::<i64>()
        .expect("parse inspection job id");

    let state = runtime.wait_for_job_state(job_id, "PENDING", Duration::from_secs(2));
    assert_eq!(state, "PENDING");
    runtime.assert_job_state_stable(job_id, "PENDING", Duration::from_millis(500));

    let details = runtime.scontrol_show_job(job_id);
    assert!(
        details.contains(&format!("JobId={job_id}")),
        "details:\n{details}"
    );
    assert!(
        details.contains("JobName=inspect-me"),
        "details:\n{details}"
    );
    assert!(details.contains("Partition=cpu"), "details:\n{details}");
    assert!(details.contains("State=PENDING"), "details:\n{details}");
    assert!(
        details.contains(&format!("Dependency={dependency}")),
        "details:\n{details}"
    );
    assert!(details.contains("WorkDir="), "details:\n{details}");
    assert!(details.contains("Command="), "details:\n{details}");
}
