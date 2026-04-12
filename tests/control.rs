mod helpers;

use std::time::Duration;

use helpers::TestRuntime;

#[test]
fn scontrol_hold_and_release_controls_pending_job() {
    let runtime = TestRuntime::new();

    let blocker_job_id = runtime
        .run_checked(&["sbatch", "--parsable", "--wrap", "sleep 2"])
        .parse::<i64>()
        .expect("parse blocker job id");
    let dependency = format!("afterok:{blocker_job_id}");
    let held_job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "--dependency",
            &dependency,
            "--wrap",
            "true",
        ])
        .parse::<i64>()
        .expect("parse held job id");

    let pending_state = runtime.wait_for_job_state(held_job_id, "PENDING", Duration::from_secs(2));
    assert_eq!(pending_state, "PENDING");

    let hold_output = runtime.run_checked(&["scontrol", "hold", "job", &held_job_id.to_string()]);
    assert!(hold_output.is_empty(), "stdout:\n{hold_output}");

    let details = runtime.scontrol_show_job(held_job_id);
    assert!(details.contains("State=PENDING"), "details:\n{details}");
    assert!(
        details.contains("Reason=JobHeldUser"),
        "details:\n{details}"
    );

    let blocker_state =
        runtime.wait_for_job_state(blocker_job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(blocker_state, "COMPLETED");
    runtime.assert_job_state_stable(held_job_id, "PENDING", Duration::from_millis(500));

    let release_output =
        runtime.run_checked(&["scontrol", "release", "job", &held_job_id.to_string()]);
    assert!(release_output.is_empty(), "stdout:\n{release_output}");

    let final_state = runtime.wait_for_job_state(held_job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(final_state, "COMPLETED");
}
