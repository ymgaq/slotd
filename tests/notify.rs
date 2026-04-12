mod helpers;

use std::fs;
use std::time::Duration;

use helpers::TestRuntime;

#[test]
fn notify_hook_runs_for_terminal_top_level_jobs() {
    let root = std::env::temp_dir().join(format!("slotd-notify-{}", std::process::id()));
    let notify_path = root.join("notify.log");
    std::fs::create_dir_all(&root).expect("create notify tempdir");
    let command = format!(
        "printf '%s|%s|%s|%s|%s\\n' \"$SLOTD_JOB_ID\" \"$SLOTD_JOB_NAME\" \"$SLOTD_JOB_STATE\" \"$SLOTD_JOB_PARTITION\" \"$SLOTD_JOB_REASON\" >> {}",
        notify_path.display()
    );
    let runtime = TestRuntime::with_env(&[("SLOTD_NOTIFY_CMD", &command)]);

    let job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "--job-name",
            "notify-check",
            "--partition",
            "cpu",
            "--wrap",
            "true",
        ])
        .parse::<i64>()
        .expect("parse job id");

    let final_state = runtime.wait_for_job_state(job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(final_state, "COMPLETED");

    runtime.wait_for_condition(Duration::from_secs(10), || notify_path.exists());
    let contents = fs::read_to_string(&notify_path).expect("read notify log");
    assert!(
        contents.contains(&format!("{job_id}|notify-check|COMPLETED|cpu|Completed")),
        "notify log:\n{contents}"
    );

    let _ = std::fs::remove_file(&notify_path);
    let _ = std::fs::remove_dir_all(&root);
}
