mod helpers;

use std::collections::HashMap;
use std::time::Duration;

use helpers::TestRuntime;

#[test]
fn array_tasks_wait_on_dependency_and_then_complete() {
    let runtime = TestRuntime::new();

    let blocker_job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "--partition",
            "cpu",
            "--wrap",
            "sleep 2",
        ])
        .parse::<i64>()
        .expect("parse blocker job id");
    let dependency = format!("afterok:{blocker_job_id}");
    let array_job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "--partition",
            "cpu",
            "--array",
            "0-1",
            "--dependency",
            &dependency,
            "--wrap",
            "true",
        ])
        .parse::<i64>()
        .expect("parse array job id");

    runtime.wait_for_condition(Duration::from_secs(5), || {
        let tasks = array_task_states(&runtime, array_job_id);
        tasks.len() == 2 && tasks.values().all(|state| state == "PENDING")
    });

    let blocker_final =
        runtime.wait_for_job_state(blocker_job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(blocker_final, "COMPLETED");

    runtime.wait_for_condition(Duration::from_secs(10), || {
        let tasks = array_task_states(&runtime, array_job_id);
        tasks.len() == 2 && tasks.values().all(|state| state == "COMPLETED")
    });
}

#[test]
fn hold_update_release_keeps_changes_and_unblocks_execution() {
    let runtime = TestRuntime::new();

    let blocker_job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "--partition",
            "cpu",
            "--wrap",
            "sleep 2",
        ])
        .parse::<i64>()
        .expect("parse blocker job id");
    let dependency = format!("afterok:{blocker_job_id}");
    let job_id = runtime
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
        .expect("parse job id");

    let pending_state = runtime.wait_for_job_state(job_id, "PENDING", Duration::from_secs(2));
    assert_eq!(pending_state, "PENDING");

    runtime.run_checked(&["scontrol", "hold", "job", &job_id.to_string()]);
    runtime.run_checked(&[
        "scontrol",
        "update",
        "job",
        &job_id.to_string(),
        "JobName=held-updated",
        "TimeLimit=00:00:05",
    ]);

    let details = runtime.scontrol_show_job(job_id);
    assert!(
        details.contains("Reason=JobHeldUser"),
        "details:\n{details}"
    );
    assert!(
        details.contains("JobName=held-updated"),
        "details:\n{details}"
    );
    assert!(
        details.contains("TimeLimit=00:00:05"),
        "details:\n{details}"
    );

    let blocker_final =
        runtime.wait_for_job_state(blocker_job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(blocker_final, "COMPLETED");
    runtime.assert_job_state_stable(job_id, "PENDING", Duration::from_millis(500));

    runtime.run_checked(&["scontrol", "release", "job", &job_id.to_string()]);
    let final_state = runtime.wait_for_job_state(job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(final_state, "COMPLETED");

    let details = runtime.scontrol_show_job(job_id);
    assert!(
        details.contains("JobName=held-updated"),
        "details:\n{details}"
    );
}

fn array_task_states(runtime: &TestRuntime, array_job_id: i64) -> HashMap<String, String> {
    runtime
        .sacct_lines("JobID,State")
        .into_iter()
        .filter_map(|line| {
            let (job_id, state) = line.split_once('|')?;
            if !job_id.starts_with(&format!("{array_job_id}_")) {
                return None;
            }
            Some((job_id.to_string(), state.to_string()))
        })
        .collect()
}
