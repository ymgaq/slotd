mod helpers;

use std::fs;
use std::process::Stdio;
use std::time::Duration;

use helpers::TestRuntime;

fn assert_signal_result(signal: &str, expected_signal_code: i32) {
    let runtime = TestRuntime::new();

    let running_job_id = runtime.submit_batch(&["sbatch", "--parsable", "--wrap", "sleep 10"]);
    runtime.wait_for_job_state(running_job_id, "RUNNING", Duration::from_secs(5));

    let output = runtime.run_checked(&["scancel", "--signal", signal, &running_job_id.to_string()]);
    assert_eq!(output, format!("Signaled job {running_job_id}"));

    let failed_state = runtime.wait_for_job_state_in(
        running_job_id,
        &["FAILED", "COMPLETING"],
        Duration::from_secs(5),
    );
    if failed_state == "COMPLETING" {
        let final_state =
            runtime.wait_for_job_state(running_job_id, "FAILED", Duration::from_secs(5));
        assert_eq!(final_state, "FAILED");
    }

    let exit_code = format!("ExitCode=0:{expected_signal_code}");
    runtime.assert_job_details_contains(running_job_id, &["State=FAILED", "Reason=Signal", &exit_code]);
}

#[test]
fn scancel_cancels_pending_job_and_keeps_it_cancelled() {
    let runtime = TestRuntime::new();

    let blocker_job_id = runtime.submit_batch(&["sbatch", "--parsable", "--wrap", "sleep 2"]);
    let dependency = format!("afterok:{blocker_job_id}");
    let pending_job_id = runtime.submit_batch(&[
        "sbatch",
        "--parsable",
        "--dependency",
        &dependency,
        "--wrap",
        "sleep 1",
    ]);

    runtime.wait_for_job_state(pending_job_id, "PENDING", Duration::from_secs(2));
    runtime.assert_job_state_stable(pending_job_id, "PENDING", Duration::from_millis(500));

    let output = runtime.run_checked(&["scancel", &pending_job_id.to_string()]);
    assert_eq!(output, format!("Cancelled job {pending_job_id}"));

    runtime.wait_for_job_state(pending_job_id, "CANCELLED", Duration::from_secs(5));
    runtime.wait_for_job_state(blocker_job_id, "COMPLETED", Duration::from_secs(10));
    runtime.assert_job_state_stable(pending_job_id, "CANCELLED", Duration::from_millis(300));
}

#[test]
fn scancel_cancels_running_job_via_completing() {
    let runtime = TestRuntime::new();

    let running_job_id = runtime.submit_batch(&["sbatch", "--parsable", "--wrap", "sleep 10"]);
    runtime.wait_for_job_state(running_job_id, "RUNNING", Duration::from_secs(5));

    let output = runtime.run_checked(&["scancel", &running_job_id.to_string()]);
    assert_eq!(output, format!("Cancelled job {running_job_id}"));

    let transition_state = runtime.wait_for_job_state_in(
        running_job_id,
        &["COMPLETING", "CANCELLED"],
        Duration::from_secs(3),
    );
    assert!(
        transition_state == "COMPLETING" || transition_state == "CANCELLED",
        "unexpected transition state: {transition_state}"
    );

    runtime.wait_for_job_state(running_job_id, "CANCELLED", Duration::from_secs(5));
    runtime.assert_job_details_contains(
        running_job_id,
        &["State=CANCELLED", "Reason=CancelledByUser"],
    );
}

#[test]
fn scancel_signal_terminates_running_job_as_failed_signal() {
    assert_signal_result("TERM", 15);
}

#[test]
fn scancel_signal_int_terminates_running_job_as_failed_signal() {
    assert_signal_result("INT", 2);
}

#[test]
fn scancel_signal_kill_terminates_running_job_as_failed_signal() {
    assert_signal_result("KILL", 9);
}

#[test]
fn scancel_signal_hup_terminates_running_job_as_failed_signal() {
    assert_signal_result("HUP", 1);
}

#[test]
fn scancel_signal_quit_terminates_running_job_as_failed_signal() {
    let runtime = TestRuntime::new();

    let running_job_id = runtime.submit_batch(&["sbatch", "--parsable", "--wrap", "sleep 10"]);
    runtime.wait_for_job_state(running_job_id, "RUNNING", Duration::from_secs(5));

    let output = runtime.run_checked(&["scancel", "--signal", "QUIT", &running_job_id.to_string()]);
    assert_eq!(output, format!("Signaled job {running_job_id}"));

    runtime.wait_for_job_state(running_job_id, "FAILED", Duration::from_secs(5));
    runtime.assert_job_details_contains(
        running_job_id,
        &["State=FAILED", "Reason=NonZeroExitCode", "ExitCode=131:0"],
    );
}

#[test]
fn scancel_step_reference_terminates_the_target_step() {
    let runtime = TestRuntime::new();
    let alloc_id_path = runtime.root_dir().join("alloc-job-id.txt");
    let step_started_path = runtime.root_dir().join("step-started.txt");
    let step_exit_path = runtime.root_dir().join("step-exit.txt");
    let finished_path = runtime.root_dir().join("allocation-finished.txt");
    let script_path = runtime.root_dir().join("nested-step-cancel.sh");

    fs::write(
        &script_path,
        format!(
            r#"#!/usr/bin/env bash
printf '%s' "$SLURM_JOB_ID" > "{alloc_id}"
set +e
SLOTD_ROOT="{slotd_root}" USER="slotd-test" "{slotd_bin}" srun bash -lc 'echo started > "{step_started}"; sleep 20'
printf '%s' "$?" > "{step_exit}"
printf done > "{finished}"
"#,
            alloc_id = alloc_id_path.display(),
            slotd_root = runtime.root_dir().display(),
            slotd_bin = env!("CARGO_BIN_EXE_slotd"),
            step_started = step_started_path.display(),
            step_exit = step_exit_path.display(),
            finished = finished_path.display(),
        ),
    )
    .expect("write nested step cancel script");

    let mut child = runtime
        .command()
        .args([
            "salloc",
            "-p",
            "cpu",
            "bash",
            script_path.to_str().expect("script path"),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn salloc");

    runtime.wait_for_condition(Duration::from_secs(10), || alloc_id_path.exists());
    let allocation_job_id = fs::read_to_string(&alloc_id_path)
        .expect("read allocation id")
        .trim()
        .parse::<i64>()
        .expect("parse allocation id");

    let step_reference = format!("{allocation_job_id}.0");
    runtime.wait_for_condition(Duration::from_secs(10), || step_started_path.exists());

    let output = runtime.run_checked(&["scancel", &step_reference]);
    assert!(!output.is_empty(), "missing scancel output");

    runtime.wait_for_condition(Duration::from_secs(10), || step_exit_path.exists());
    let status = child.wait().expect("wait for salloc");
    assert!(status.success(), "salloc exited with status {status}");
    assert!(finished_path.exists(), "allocation script did not finish");

    let step_exit = fs::read_to_string(&step_exit_path).expect("read step exit");
    assert_ne!(step_exit.trim(), "0", "step exit unexpectedly succeeded");

    let allocation_final = runtime.wait_for_job_state_in(
        allocation_job_id,
        &["FAILED", "COMPLETED"],
        Duration::from_secs(10),
    );
    assert!(
        matches!(allocation_final.as_str(), "FAILED" | "COMPLETED"),
        "unexpected allocation state: {allocation_final}"
    );
    let sacct = runtime.run_checked(&["sacct", "-P", "-n", "-o", "JobID,State"]);
    assert!(
        sacct
            .lines()
            .any(|line| line.trim() == format!("{allocation_job_id}.0|FAILED")),
        "sacct:\n{sacct}"
    );
}
