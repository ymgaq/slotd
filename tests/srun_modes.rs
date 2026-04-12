mod helpers;

use std::time::Duration;

use helpers::TestRuntime;

#[test]
fn srun_no_wait_submits_background_run_job() {
    let runtime = TestRuntime::new();

    let output = runtime.run_checked(&["srun", "--no-wait", "--", "true"]);
    let job_id = output
        .strip_prefix("Submitted run job ")
        .expect("submitted job prefix")
        .parse::<i64>()
        .expect("parse run job id");

    let final_state = runtime.wait_for_job_state(job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(final_state, "COMPLETED");
}

#[test]
fn salloc_immediate_fails_when_resources_are_unavailable() {
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

    let output = runtime.run_output(&["salloc", "--immediate", "-c", "1", "/bin/true"]);
    assert!(
        !output.status.success(),
        "salloc unexpectedly succeeded\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("resources are not currently available for --immediate salloc"),
        "stderr:\n{stderr}"
    );
}
