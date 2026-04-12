mod helpers;

use std::time::Duration;

use helpers::TestRuntime;

#[test]
fn squeue_filters_views_and_sorting_work_for_active_jobs() {
    let runtime = TestRuntime::new();

    let blocker_job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "--partition",
            "cpu",
            "--wrap",
            "sleep 5",
        ])
        .parse::<i64>()
        .expect("parse blocker job id");
    let dependency = format!("afterok:{blocker_job_id}");
    let pending_job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "--partition",
            "cpu",
            "--dependency",
            &dependency,
            "--wrap",
            "true",
        ])
        .parse::<i64>()
        .expect("parse pending job id");

    let running_state = runtime.wait_for_job_state(blocker_job_id, "RUNNING", Duration::from_secs(5));
    assert_eq!(running_state, "RUNNING");
    let pending_state = runtime.wait_for_job_state(pending_job_id, "PENDING", Duration::from_secs(2));
    assert_eq!(pending_state, "PENDING");

    let listed = runtime.run_checked(&[
        "squeue",
        "--noheader",
        "-j",
        &format!("{blocker_job_id},{pending_job_id}"),
        "-o",
        "JobID,State,Partition",
    ]);
    assert!(listed.contains(&blocker_job_id.to_string()), "squeue:\n{listed}");
    assert!(listed.contains(&pending_job_id.to_string()), "squeue:\n{listed}");
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
    assert!(!pending_only.contains(&blocker_job_id.to_string()), "squeue:\n{pending_only}");
    assert!(pending_only.contains(&pending_job_id.to_string()), "squeue:\n{pending_only}");

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
    assert!(partition_filtered.contains(&blocker_job_id.to_string()), "squeue:\n{partition_filtered}");
    assert!(partition_filtered.contains("cpu"), "squeue:\n{partition_filtered}");

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
    assert!(long_view.contains(&blocker_job_id.to_string()), "squeue:\n{long_view}");
    assert!(long_view.contains("512M"), "squeue:\n{long_view}");

    let start_view = runtime.run_checked(&[
        "squeue",
        "--noheader",
        "--start",
        "-j",
        &pending_job_id.to_string(),
    ]);
    assert!(start_view.contains(&pending_job_id.to_string()), "squeue:\n{start_view}");
}

#[test]
fn squeue_all_and_array_views_show_historical_and_array_jobs() {
    let runtime = TestRuntime::new();

    let completed_job_id = runtime
        .run_checked(&["sbatch", "--parsable", "--partition", "cpu", "--wrap", "true"])
        .parse::<i64>()
        .expect("parse completed job id");
    let completed_state =
        runtime.wait_for_job_state(completed_job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(completed_state, "COMPLETED");

    let active_view = runtime.run_checked(&[
        "squeue",
        "--noheader",
        "-j",
        &completed_job_id.to_string(),
    ]);
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
    assert!(all_view.contains(&completed_job_id.to_string()), "squeue:\n{all_view}");
    assert!(all_view.contains("CD"), "squeue:\n{all_view}");

    let array_job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "--partition",
            "cpu",
            "--array",
            "0-1%1",
            "--wrap",
            "sleep 2",
        ])
        .parse::<i64>()
        .expect("parse array job id");
    runtime.wait_for_condition(Duration::from_secs(5), || {
        let output = runtime.run_checked(&["squeue", "--array", "--noheader", "-o", "JobID,State"]);
        output.contains(&format!("{array_job_id}_0")) || output.contains(&format!("{array_job_id}_1"))
    });
}

#[test]
fn sacct_filters_formats_and_time_bounds_work() {
    let runtime = TestRuntime::new();

    let completed_job_id = runtime
        .run_checked(&["sbatch", "--parsable", "--partition", "cpu", "--wrap", "true"])
        .parse::<i64>()
        .expect("parse completed job id");
    let failed_job_id = runtime
        .run_checked(&["sbatch", "--parsable", "--partition", "cpu", "--wrap", "exit 1"])
        .parse::<i64>()
        .expect("parse failed job id");

    let completed_state =
        runtime.wait_for_job_state(completed_job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(completed_state, "COMPLETED");
    let failed_state = runtime.wait_for_job_state(failed_job_id, "FAILED", Duration::from_secs(10));
    assert_eq!(failed_state, "FAILED");

    let by_ids = runtime.run_checked(&[
        "sacct",
        "-P",
        "-n",
        "-j",
        &format!("{completed_job_id},{failed_job_id}"),
        "-o",
        "JobID,State,Partition",
    ]);
    assert!(by_ids.contains(&format!("{completed_job_id}|COMPLETED|cpu")), "sacct:\n{by_ids}");
    assert!(by_ids.contains(&format!("{failed_job_id}|FAILED|cpu")), "sacct:\n{by_ids}");

    let by_state = runtime.run_checked(&[
        "sacct",
        "-P",
        "-n",
        "-s",
        "COMPLETED",
        "-j",
        &format!("{completed_job_id},{failed_job_id}"),
        "-o",
        "JobID,State",
    ]);
    assert!(by_state.contains(&format!("{completed_job_id}|COMPLETED")), "sacct:\n{by_state}");
    assert!(!by_state.contains(&failed_job_id.to_string()), "sacct:\n{by_state}");

    let by_user_partition = runtime.run_checked(&[
        "sacct",
        "-P",
        "-n",
        "-u",
        "slotd-test",
        "-p",
        "cpu",
        "-j",
        &completed_job_id.to_string(),
        "-o",
        "JobID,User,Partition",
    ]);
    assert!(
        by_user_partition.contains(&format!("{completed_job_id}|slotd-test|cpu")),
        "sacct:\n{by_user_partition}"
    );

    let start_excluded = runtime.run_checked(&[
        "sacct",
        "-P",
        "-n",
        "-S",
        "2100-01-01",
        "-j",
        &completed_job_id.to_string(),
        "-o",
        "JobID,State",
    ]);
    assert!(start_excluded.is_empty(), "sacct:\n{start_excluded}");

    let end_excluded = runtime.run_checked(&[
        "sacct",
        "-P",
        "-n",
        "-E",
        "2000-01-01",
        "-j",
        &completed_job_id.to_string(),
        "-o",
        "JobID,State",
    ]);
    assert!(end_excluded.is_empty(), "sacct:\n{end_excluded}");
}

#[test]
fn sacct_shows_allocation_and_step_records() {
    let runtime = TestRuntime::new();

    let output = runtime.run_output(&["srun", "-p", "cpu", "--", "true"]);
    assert!(
        output.status.success(),
        "srun failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let sacct = runtime.run_checked(&["sacct", "-P", "-n", "-o", "JobID,State"]);
    assert!(
        sacct.contains("1|COMPLETED"),
        "sacct:\n{sacct}"
    );
    assert!(
        sacct.contains("1.0|COMPLETED"),
        "sacct:\n{sacct}"
    );
}
