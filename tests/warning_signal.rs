mod helpers;

use std::fs;
use std::time::Duration;

use helpers::TestRuntime;

#[test]
fn sbatch_warning_signal_is_delivered_before_time_limit() {
    let runtime = TestRuntime::new();
    let signal_path = runtime.root_dir().join("warning-signal.txt");

    let job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "--time",
            "00:00:04",
            "--signal",
            "USR1@2",
            "--wrap",
            &format!(
                "bash -lc 'trap \"echo warned > {} ; exit 0\" USR1; sleep 10'",
                signal_path.display()
            ),
        ])
        .parse::<i64>()
        .expect("parse job id");

    runtime.wait_for_condition(Duration::from_secs(10), || signal_path.exists());

    let final_state =
        runtime.wait_for_job_state_in(job_id, &["COMPLETED", "FAILED"], Duration::from_secs(10));
    assert!(
        matches!(final_state.as_str(), "COMPLETED" | "FAILED"),
        "unexpected final state: {final_state}"
    );

    let signal_contents = fs::read_to_string(&signal_path).expect("read warning signal file");
    assert_eq!(signal_contents.trim(), "warned");

    let details = runtime.scontrol_show_job(job_id);
    assert!(
        details.contains("State=COMPLETED") || details.contains("State=FAILED"),
        "details:\n{details}"
    );
}
