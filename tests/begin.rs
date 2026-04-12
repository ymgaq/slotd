mod helpers;

use std::time::Duration;

use helpers::TestRuntime;

#[test]
fn begin_time_keeps_job_pending_until_requested_time() {
    let runtime = TestRuntime::new();

    let job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "--begin",
            "now+00:00:03",
            "--wrap",
            "true",
        ])
        .parse::<i64>()
        .expect("parse begin job id");

    let pending_state = runtime.wait_for_job_state(job_id, "PENDING", Duration::from_secs(2));
    assert_eq!(pending_state, "PENDING");

    let details = runtime.scontrol_show_job(job_id);
    assert!(details.contains(&format!("JobId={job_id}")), "details:\n{details}");
    assert!(details.contains("State=PENDING"), "details:\n{details}");
    assert!(details.contains("Reason=BeginTime"), "details:\n{details}");

    runtime.assert_job_state_stable(job_id, "PENDING", Duration::from_millis(800));

    let final_state = runtime.wait_for_job_state(job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(final_state, "COMPLETED");
}
