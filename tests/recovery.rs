mod helpers;

use std::time::Duration;

use helpers::TestRuntime;

#[test]
fn daemon_restart_adopts_running_job_and_records_terminal_state() {
    let mut runtime = TestRuntime::new();

    let job_id = runtime
        .run_checked(&["sbatch", "--parsable", "--wrap", "sleep 2"])
        .parse::<i64>()
        .expect("parse job id");

    let running_state = runtime.wait_for_job_state(job_id, "RUNNING", Duration::from_secs(5));
    assert_eq!(running_state, "RUNNING");

    runtime.restart_daemon();

    let final_state = runtime.wait_for_job_state_in(
        job_id,
        &["COMPLETED", "FAILED"],
        Duration::from_secs(10),
    );
    let details = runtime.scontrol_show_job(job_id);
    assert!(
        details.contains(&format!("JobId={job_id}")),
        "details:\n{details}"
    );
    assert!(
        details.contains(&format!("State={final_state}")),
        "details:\n{details}"
    );
    assert!(
        details.contains("Reason=Completed") || details.contains("Reason=LostAfterRestart"),
        "details:\n{details}"
    );
}

#[test]
fn daemon_restart_recovers_job_that_finished_while_daemon_was_down() {
    let mut runtime = TestRuntime::new();

    let job_id = runtime
        .run_checked(&["sbatch", "--parsable", "--wrap", "sleep 1"])
        .parse::<i64>()
        .expect("parse job id");

    let running_state = runtime.wait_for_job_state(job_id, "RUNNING", Duration::from_secs(5));
    assert_eq!(running_state, "RUNNING");

    runtime.stop_daemon();
    std::thread::sleep(Duration::from_secs(2));
    runtime.restart_daemon();

    let final_state =
        runtime.wait_for_job_state_in(job_id, &["COMPLETED", "FAILED"], Duration::from_secs(5));
    let details = runtime.scontrol_show_job(job_id);
    assert!(
        details.contains(&format!("JobId={job_id}")),
        "details:\n{details}"
    );
    assert!(
        details.contains(&format!("State={final_state}")),
        "details:\n{details}"
    );
    assert!(
        details.contains("Reason=Completed") || details.contains("Reason=LostAfterRestart"),
        "details:\n{details}"
    );
}
