mod helpers;

use std::time::Duration;

use helpers::TestRuntime;

#[test]
fn squeue_filters_views_and_sorting_work_for_active_jobs() {
    let runtime = TestRuntime::new();

    let blocker_job_id = runtime.submit_batch(&[
        "sbatch",
        "--parsable",
        "--partition",
        "cpu",
        "--wrap",
        "sleep 5",
    ]);
    let dependency = format!("afterok:{blocker_job_id}");
    let pending_job_id = runtime.submit_batch(&[
        "sbatch",
        "--parsable",
        "--partition",
        "cpu",
        "--dependency",
        &dependency,
        "--wrap",
        "true",
    ]);

    runtime.wait_for_job_state(blocker_job_id, "RUNNING", Duration::from_secs(5));
    runtime.wait_for_job_state(pending_job_id, "PENDING", Duration::from_secs(2));

    let listed = runtime.run_checked(&[
        "squeue",
        "--noheader",
        "-j",
        &format!("{blocker_job_id},{pending_job_id}"),
        "-o",
        "JobID,State,Partition",
    ]);
    assert!(
        listed.contains(&blocker_job_id.to_string()),
        "squeue:\n{listed}"
    );
    assert!(
        listed.contains(&pending_job_id.to_string()),
        "squeue:\n{listed}"
    );
    assert!(listed.contains("R"), "squeue:\n{listed}");
    assert!(listed.contains("PD"), "squeue:\n{listed}");

    let pending_only = runtime.run_checked(&[
        "squeue",
        "--noheader",
        "-t",
        "PENDING",
        "-j",
        &format!("{blocker_job_id},{pending_job_id}"),
        "-o",
        "JobID,State",
    ]);
    assert!(
        !pending_only.contains(&blocker_job_id.to_string()),
        "squeue:\n{pending_only}"
    );
    assert!(
        pending_only.contains(&pending_job_id.to_string()),
        "squeue:\n{pending_only}"
    );

    let partition_filtered = runtime.run_checked(&[
        "squeue",
        "--noheader",
        "-p",
        "cpu",
        "-j",
        &format!("{blocker_job_id},{pending_job_id}"),
        "-o",
        "JobID,Partition",
    ]);
    assert!(
        partition_filtered.contains(&blocker_job_id.to_string()),
        "squeue:\n{partition_filtered}"
    );
    assert!(
        partition_filtered.contains("cpu"),
        "squeue:\n{partition_filtered}"
    );

    let user_filtered = runtime.run_checked(&[
        "squeue",
        "--noheader",
        "-u",
        "slotd-test",
        "-j",
        &format!("{blocker_job_id},{pending_job_id}"),
        "-o",
        "JobID,User",
    ]);
    assert!(
        user_filtered.contains(&blocker_job_id.to_string()),
        "squeue:\n{user_filtered}"
    );
    assert!(
        user_filtered.contains("slotd-test"),
        "squeue:\n{user_filtered}"
    );

    let sorted = runtime.run_checked(&[
        "squeue",
        "--noheader",
        "--sort=-jobid",
        "-j",
        &format!("{blocker_job_id},{pending_job_id}"),
        "-o",
        "JobID",
    ]);
    let first_job = sorted
        .lines()
        .find(|line| !line.trim().is_empty())
        .and_then(|line| line.split_whitespace().next())
        .expect("sorted first job");
    assert_eq!(first_job, pending_job_id.to_string(), "squeue:\n{sorted}");

    let long_view = runtime.run_checked(&[
        "squeue",
        "--noheader",
        "-l",
        "-j",
        &blocker_job_id.to_string(),
    ]);
    assert!(
        long_view.contains(&blocker_job_id.to_string()),
        "squeue:\n{long_view}"
    );
    assert!(long_view.contains("512M"), "squeue:\n{long_view}");

    let start_view = runtime.run_checked(&[
        "squeue",
        "--noheader",
        "--start",
        "-j",
        &pending_job_id.to_string(),
    ]);
    assert!(
        start_view.contains(&pending_job_id.to_string()),
        "squeue:\n{start_view}"
    );
}

#[test]
fn squeue_all_and_array_views_show_historical_and_array_jobs() {
    let runtime = TestRuntime::new();

    let completed_job_id = runtime.submit_batch(&[
        "sbatch",
        "--parsable",
        "--partition",
        "cpu",
        "--wrap",
        "true",
    ]);
    runtime.wait_for_job_state(completed_job_id, "COMPLETED", Duration::from_secs(10));

    let active_view =
        runtime.run_checked(&["squeue", "--noheader", "-j", &completed_job_id.to_string()]);
    assert!(active_view.is_empty(), "squeue:\n{active_view}");

    let all_view = runtime.run_checked(&[
        "squeue",
        "--all",
        "--noheader",
        "-j",
        &completed_job_id.to_string(),
        "-o",
        "JobID,State",
    ]);
    assert!(
        all_view.contains(&completed_job_id.to_string()),
        "squeue:\n{all_view}"
    );
    assert!(all_view.contains("CD"), "squeue:\n{all_view}");

    let array_job_id = runtime.submit_batch(&[
        "sbatch",
        "--parsable",
        "--partition",
        "cpu",
        "--array",
        "0-1%1",
        "--wrap",
        "sleep 2",
    ]);
    runtime.wait_for_condition(Duration::from_secs(5), || {
        let output = runtime.run_checked(&["squeue", "--array", "--noheader", "-o", "JobID,State"]);
        output.contains(&format!("{array_job_id}_0"))
            || output.contains(&format!("{array_job_id}_1"))
    });
}
