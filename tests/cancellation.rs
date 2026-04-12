mod helpers;

use std::time::Duration;

use helpers::TestRuntime;

#[test]
fn scancel_cancels_pending_job_and_keeps_it_cancelled() {
    let runtime = TestRuntime::new();

    let blocker_job_id = runtime
        .run_checked(&["sbatch", "--parsable", "--wrap", "sleep 2"])
        .parse::<i64>()
        .expect("parse blocker job id");
    let dependency = format!("afterok:{blocker_job_id}");
    let pending_job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "--dependency",
            &dependency,
            "--wrap",
            "sleep 1",
        ])
        .parse::<i64>()
        .expect("parse pending job id");

    let state = runtime.wait_for_job_state(pending_job_id, "PENDING", Duration::from_secs(2));
    assert_eq!(state, "PENDING");
    runtime.assert_job_state_stable(pending_job_id, "PENDING", Duration::from_millis(500));

    let output = runtime.run_checked(&["scancel", &pending_job_id.to_string()]);
    assert_eq!(output, format!("Cancelled job {pending_job_id}"));

    let cancelled_state =
        runtime.wait_for_job_state(pending_job_id, "CANCELLED", Duration::from_secs(5));
    assert_eq!(cancelled_state, "CANCELLED");

    let blocker_state =
        runtime.wait_for_job_state(blocker_job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(blocker_state, "COMPLETED");
    runtime.assert_job_state_stable(pending_job_id, "CANCELLED", Duration::from_millis(300));
}

#[test]
fn scancel_cancels_running_job_via_completing() {
    let runtime = TestRuntime::new();

    let running_job_id = runtime
        .run_checked(&["sbatch", "--parsable", "--wrap", "sleep 10"])
        .parse::<i64>()
        .expect("parse running job id");

    let running_state = runtime.wait_for_job_state(running_job_id, "RUNNING", Duration::from_secs(5));
    assert_eq!(running_state, "RUNNING");

    let output = runtime.run_checked(&["scancel", &running_job_id.to_string()]);
    assert_eq!(output, format!("Cancelled job {running_job_id}"));

    let transition_state = runtime.wait_for_job_state_in(
        running_job_id,
        &["COMPLETING", "CANCELLED"],
        Duration::from_secs(3),
    );
    assert!(
        transition_state == "COMPLETING" || transition_state == "CANCELLED",
        "unexpected transition state: {transition_state}"
    );

    let cancelled_state =
        runtime.wait_for_job_state(running_job_id, "CANCELLED", Duration::from_secs(5));
    assert_eq!(cancelled_state, "CANCELLED");

    let details = runtime.scontrol_show_job(running_job_id);
    assert!(details.contains("State=CANCELLED"), "details:\n{details}");
    assert!(details.contains("Reason=CancelledByUser"), "details:\n{details}");
}

#[test]
fn scancel_signal_terminates_running_job_as_failed_signal() {
    let runtime = TestRuntime::new();

    let running_job_id = runtime
        .run_checked(&["sbatch", "--parsable", "--wrap", "sleep 10"])
        .parse::<i64>()
        .expect("parse running job id");

    let running_state = runtime.wait_for_job_state(running_job_id, "RUNNING", Duration::from_secs(5));
    assert_eq!(running_state, "RUNNING");

    let output = runtime.run_checked(&["scancel", "--signal", "TERM", &running_job_id.to_string()]);
    assert_eq!(output, format!("Signaled job {running_job_id}"));

    let failed_state = runtime.wait_for_job_state_in(
        running_job_id,
        &["FAILED", "COMPLETING"],
        Duration::from_secs(5),
    );
    if failed_state == "COMPLETING" {
        let final_state =
            runtime.wait_for_job_state(running_job_id, "FAILED", Duration::from_secs(5));
        assert_eq!(final_state, "FAILED");
    }

    let details = runtime.scontrol_show_job(running_job_id);
    assert!(details.contains("State=FAILED"), "details:\n{details}");
    assert!(details.contains("Reason=Signal"), "details:\n{details}");
    assert!(details.contains("ExitCode=0:15"), "details:\n{details}");
}
