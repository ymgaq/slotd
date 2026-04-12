mod helpers;

use std::time::Duration;

use helpers::TestRuntime;

#[test]
fn sacct_filters_formats_and_time_bounds_work() {
    let runtime = TestRuntime::new();

    let completed_job_id = runtime.submit_batch(&[
        "sbatch",
        "--parsable",
        "--partition",
        "cpu",
        "--wrap",
        "true",
    ]);
    let failed_job_id = runtime.submit_batch(&[
        "sbatch",
        "--parsable",
        "--partition",
        "cpu",
        "--wrap",
        "exit 1",
    ]);

    runtime.wait_for_job_state(completed_job_id, "COMPLETED", Duration::from_secs(10));
    runtime.wait_for_job_state(failed_job_id, "FAILED", Duration::from_secs(10));

    let by_ids = runtime.run_checked(&[
        "sacct",
        "-P",
        "-n",
        "-j",
        &format!("{completed_job_id},{failed_job_id}"),
        "-o",
        "JobID,State,Partition",
    ]);
    assert!(
        by_ids.contains(&format!("{completed_job_id}|COMPLETED|cpu")),
        "sacct:\n{by_ids}"
    );
    assert!(
        by_ids.contains(&format!("{failed_job_id}|FAILED|cpu")),
        "sacct:\n{by_ids}"
    );

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
    assert!(
        by_state.contains(&format!("{completed_job_id}|COMPLETED")),
        "sacct:\n{by_state}"
    );
    assert!(
        !by_state.contains(&failed_job_id.to_string()),
        "sacct:\n{by_state}"
    );

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
    assert!(sacct.contains("1|COMPLETED"), "sacct:\n{sacct}");
    assert!(sacct.contains("1.0|COMPLETED"), "sacct:\n{sacct}");
}

#[test]
fn sacct_supports_richer_field_combinations() {
    let runtime = TestRuntime::new();

    let job_id = runtime.submit_batch(&[
        "sbatch",
        "--parsable",
        "--partition",
        "cpu",
        "--job-name",
        "richacct",
        "--wrap",
        "echo rich",
    ]);

    runtime.wait_for_job_state(job_id, "COMPLETED", Duration::from_secs(10));

    let output = runtime.run_checked(&[
        "sacct",
        "-P",
        "-n",
        "-j",
        &job_id.to_string(),
        "-o",
        "JobID,JobName,Reason,ExitCode,Elapsed,ReqMem,ReqTres,AllocTres,NodeList,Submit,Start,End,WorkDir,BatchFlag",
    ]);
    let line = output
        .lines()
        .find(|line| line.starts_with(&job_id.to_string()))
        .expect("top-level sacct line");
    let fields = line.split('|').collect::<Vec<_>>();
    assert_eq!(fields[0], job_id.to_string());
    assert_eq!(fields[1], "richacct");
    assert_eq!(fields[2], "Completed");
    assert_eq!(fields[3], "0:0");
    assert!(!fields[4].is_empty(), "sacct:\n{output}");
    assert!(fields[5].contains('M'), "sacct:\n{output}");
    assert!(fields[6].contains("cpu="), "sacct:\n{output}");
    assert!(fields[7].contains("cpu="), "sacct:\n{output}");
    assert!(!fields[8].is_empty(), "sacct:\n{output}");
    assert!(!fields[9].is_empty(), "sacct:\n{output}");
    assert!(!fields[10].is_empty(), "sacct:\n{output}");
    assert!(!fields[11].is_empty(), "sacct:\n{output}");
    assert!(
        fields[12].contains(runtime.root_dir().to_str().expect("root dir")),
        "sacct:\n{output}"
    );
    assert_eq!(fields[13], "1");
}
