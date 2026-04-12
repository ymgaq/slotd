mod helpers;

use std::time::Duration;

use helpers::TestRuntime;

#[test]
fn job_exceeding_time_limit_finishes_as_timeout() {
    let runtime = TestRuntime::new();

    let job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "--time",
            "00:00:01",
            "--wrap",
            "sleep 5",
        ])
        .parse::<i64>()
        .expect("parse timed job id");

    let running_state = runtime.wait_for_job_state(job_id, "RUNNING", Duration::from_secs(5));
    assert_eq!(running_state, "RUNNING");

    let final_state = runtime.wait_for_job_state(job_id, "TIMEOUT", Duration::from_secs(10));
    assert_eq!(final_state, "TIMEOUT");

    let details = runtime.scontrol_show_job(job_id);
    assert!(
        details.contains(&format!("JobId={job_id}")),
        "details:\n{details}"
    );
    assert!(details.contains("State=TIMEOUT"), "details:\n{details}");
    assert!(details.contains("Reason=TimeLimit"), "details:\n{details}");
    assert!(
        details.contains("TimeLimit=00:00:01"),
        "details:\n{details}"
    );
}
