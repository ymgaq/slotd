mod helpers;

use std::time::Duration;

use helpers::TestRuntime;

#[test]
fn srun_immediate_fails_when_running_job_holds_all_cpus() {
    let runtime = TestRuntime::new();
    let total_cpus = std::thread::available_parallelism()
        .expect("available parallelism")
        .get()
        .to_string();

    let blocker_job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "--cpus-per-task",
            &total_cpus,
            "--wrap",
            "sleep 5",
        ])
        .parse::<i64>()
        .expect("parse blocker job id");

    let running_state = runtime.wait_for_job_state(blocker_job_id, "RUNNING", Duration::from_secs(5));
    assert_eq!(running_state, "RUNNING");

    let output = runtime.run_output(&[
        "srun",
        "--immediate",
        "--cpus-per-task",
        "1",
        "--",
        "true",
    ]);
    assert!(
        !output.status.success(),
        "srun unexpectedly succeeded\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("resources are not currently available for --immediate srun"),
        "stderr:\n{stderr}"
    );

    let blocker_state =
        runtime.wait_for_job_state(blocker_job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(blocker_state, "COMPLETED");
}
