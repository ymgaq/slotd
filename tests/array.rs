mod helpers;

use std::collections::HashSet;
use std::time::Duration;

use helpers::TestRuntime;

#[test]
fn array_job_expands_all_tasks_and_completes() {
    let runtime = TestRuntime::new();

    let array_job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "--array",
            "0-2",
            "--wrap",
            "sleep 1",
        ])
        .parse::<i64>()
        .expect("parse array job id");

    runtime.wait_for_condition(Duration::from_secs(10), || {
        array_task_states(&runtime, array_job_id).len() == 3
    });

    runtime.wait_for_condition(Duration::from_secs(10), || {
        let tasks = array_task_states(&runtime, array_job_id);
        tasks.len() == 3 && tasks.values().all(|state| state == "COMPLETED")
    });

    let tasks = array_task_states(&runtime, array_job_id);
    let job_ids = tasks.keys().cloned().collect::<HashSet<_>>();
    assert_eq!(job_ids.len(), 3, "tasks: {tasks:?}");
    assert!(
        job_ids.contains(&format!("{array_job_id}_0")),
        "tasks: {tasks:?}"
    );
    assert!(
        job_ids.contains(&format!("{array_job_id}_1")),
        "tasks: {tasks:?}"
    );
    assert!(
        job_ids.contains(&format!("{array_job_id}_2")),
        "tasks: {tasks:?}"
    );
}

#[test]
fn array_job_respects_concurrency_limit() {
    let runtime = TestRuntime::new();

    let array_job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "--array",
            "0-3%1",
            "--wrap",
            "sleep 1",
        ])
        .parse::<i64>()
        .expect("parse array job id");

    runtime.wait_for_condition(Duration::from_secs(5), || {
        let tasks = array_task_rows(&runtime, array_job_id);
        tasks.len() == 4
            && tasks.values().filter(|state| *state == "RUNNING").count() == 1
            && tasks.values().any(|state| state == "PENDING")
    });

    runtime.wait_for_condition(Duration::from_secs(10), || {
        let tasks = array_task_rows(&runtime, array_job_id);
        tasks.len() == 4 && tasks.values().all(|state| state == "COMPLETED")
    });
}

fn array_task_states(
    runtime: &TestRuntime,
    array_job_id: i64,
) -> std::collections::HashMap<String, String> {
    array_task_rows(runtime, array_job_id)
}

fn array_task_rows(
    runtime: &TestRuntime,
    array_job_id: i64,
) -> std::collections::HashMap<String, String> {
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
