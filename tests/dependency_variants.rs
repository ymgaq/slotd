mod helpers;

use std::collections::HashSet;
use std::time::Duration;

use helpers::TestRuntime;

#[test]
fn after_dependency_releases_once_prerequisite_has_started() {
    let runtime = TestRuntime::new();

    let blocker_job_id = runtime
        .run_checked(&["sbatch", "--parsable", "--partition", "cpu", "--wrap", "sleep 3"])
        .parse::<i64>()
        .expect("parse blocker job id");
    let blocker_state = runtime.wait_for_job_state(blocker_job_id, "RUNNING", Duration::from_secs(5));
    assert_eq!(blocker_state, "RUNNING");

    let dependency = format!("after:{blocker_job_id}");
    let dependent_job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "--partition",
            "gpu",
            "--dependency",
            &dependency,
            "--wrap",
            "true",
        ])
        .parse::<i64>()
        .expect("parse dependent job id");

    let dependent_state =
        runtime.wait_for_job_state(dependent_job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(dependent_state, "COMPLETED");

    let blocker_final = runtime.wait_for_job_state(blocker_job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(blocker_final, "COMPLETED");
}

#[test]
fn afterany_dependency_releases_after_failure() {
    let runtime = TestRuntime::new();

    let failed_job_id = runtime
        .run_checked(&["sbatch", "--parsable", "--partition", "cpu", "--wrap", "exit 1"])
        .parse::<i64>()
        .expect("parse failed job id");
    let dependency = format!("afterany:{failed_job_id}");
    let dependent_job_id = runtime
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
        .expect("parse dependent job id");

    let failed_state = runtime.wait_for_job_state(failed_job_id, "FAILED", Duration::from_secs(10));
    assert_eq!(failed_state, "FAILED");
    let dependent_state =
        runtime.wait_for_job_state(dependent_job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(dependent_state, "COMPLETED");
}

#[test]
fn afternotok_dependency_requires_unsuccessful_prerequisite() {
    let runtime = TestRuntime::new();

    let failed_job_id = runtime
        .run_checked(&["sbatch", "--parsable", "--partition", "cpu", "--wrap", "exit 1"])
        .parse::<i64>()
        .expect("parse failed job id");
    let dependency = format!("afternotok:{failed_job_id}");
    let dependent_job_id = runtime
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
        .expect("parse dependent job id");

    let failed_state = runtime.wait_for_job_state(failed_job_id, "FAILED", Duration::from_secs(10));
    assert_eq!(failed_state, "FAILED");
    let dependent_state =
        runtime.wait_for_job_state(dependent_job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(dependent_state, "COMPLETED");
}

#[test]
fn singleton_dependency_waits_for_active_job_with_same_name() {
    let runtime = TestRuntime::new();

    let first_job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "--partition",
            "cpu",
            "--job-name",
            "singleton-case",
            "--wrap",
            "sleep 2",
        ])
        .parse::<i64>()
        .expect("parse first job id");
    let first_state = runtime.wait_for_job_state(first_job_id, "RUNNING", Duration::from_secs(5));
    assert_eq!(first_state, "RUNNING");

    let second_job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "--partition",
            "gpu",
            "--job-name",
            "singleton-case",
            "--dependency",
            "singleton",
            "--wrap",
            "true",
        ])
        .parse::<i64>()
        .expect("parse second job id");

    let pending_state = runtime.wait_for_job_state(second_job_id, "PENDING", Duration::from_secs(2));
    assert_eq!(pending_state, "PENDING");
    runtime.assert_job_state_stable(second_job_id, "PENDING", Duration::from_millis(500));

    let first_final = runtime.wait_for_job_state(first_job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(first_final, "COMPLETED");
    let second_final = runtime.wait_for_job_state(second_job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(second_final, "COMPLETED");
}

#[test]
fn array_spec_supports_steps_and_mixed_segments() {
    let runtime = TestRuntime::new();

    let array_job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "--partition",
            "cpu",
            "--array",
            "1-5:2,8",
            "--wrap",
            "true",
        ])
        .parse::<i64>()
        .expect("parse array job id");

    runtime.wait_for_condition(Duration::from_secs(10), || {
        let tasks = array_task_ids(&runtime, array_job_id);
        tasks.len() == 4 && tasks == HashSet::from([
            format!("{array_job_id}_1"),
            format!("{array_job_id}_3"),
            format!("{array_job_id}_5"),
            format!("{array_job_id}_8"),
        ])
    });
}

fn array_task_ids(runtime: &TestRuntime, array_job_id: i64) -> HashSet<String> {
    runtime
        .sacct_lines("JobID,State")
        .into_iter()
        .filter_map(|line| {
            let (job_id, _) = line.split_once('|')?;
            if !job_id.starts_with(&format!("{array_job_id}_")) {
                return None;
            }
            Some(job_id.to_string())
        })
        .collect()
}
