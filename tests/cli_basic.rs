mod helpers;

use std::time::Duration;

use helpers::TestRuntime;

#[test]
fn sbatch_wrap_job_reaches_completed_state() {
    let runtime = TestRuntime::new();

    let job_id = runtime
        .run_checked(&["sbatch", "--parsable", "--wrap", "true"])
        .parse::<i64>()
        .expect("parse job id");

    let state = runtime.wait_for_job_state(job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(state, "COMPLETED");
}
