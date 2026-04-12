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
