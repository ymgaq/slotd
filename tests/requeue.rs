mod helpers;

use std::fs;
use std::time::Duration;

use helpers::TestRuntime;

#[test]
fn requeue_retries_failed_job_once_and_only_once() {
    let runtime = TestRuntime::new();
    let attempt_log = runtime.root_dir().join("attempts.log");
    let wrap = format!(
        "echo attempt >> {}; exit 1",
        attempt_log.display()
    );

    let job_id = runtime
        .run_checked(&["sbatch", "--parsable", "--requeue", "--wrap", &wrap])
        .parse::<i64>()
        .expect("parse requeue job id");

    let state = runtime.wait_for_job_state(job_id, "FAILED", Duration::from_secs(10));
    assert_eq!(state, "FAILED");

    let attempts = fs::read_to_string(&attempt_log).expect("read attempt log");
    let attempt_count = attempts.lines().count();
    assert_eq!(attempt_count, 2, "attempt log contents:\n{attempts}");
}
