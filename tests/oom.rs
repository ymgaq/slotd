mod helpers;

use std::fs;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use helpers::TestRuntime;

fn unique_fake_cgroup_base() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir()
        .join(format!("slotd-fake-cgroup-{}-{nanos}", std::process::id()))
        .to_string_lossy()
        .to_string()
}

#[test]
fn sbatch_marks_job_out_of_memory_when_cgroup_reports_oom() {
    let cgroup_base = unique_fake_cgroup_base();
    let runtime = TestRuntime::with_env(&[("SLOTD_CGROUP_BASE", &cgroup_base)]);

    let job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "--mem",
            "64M",
            "--wrap",
            &format!(
                "bash -lc 'printf \"oom_kill 1\\n\" > \"{}/slotd-$SLURM_JOB_ID/memory.events\"'",
                cgroup_base
            ),
        ])
        .parse::<i64>()
        .expect("parse job id");

    let final_state = runtime.wait_for_job_state(job_id, "OUT_OF_MEMORY", Duration::from_secs(10));
    assert_eq!(final_state, "OUT_OF_MEMORY");

    let details = runtime.scontrol_show_job(job_id);
    assert!(
        details.contains("State=OUT_OF_MEMORY"),
        "details:\n{details}"
    );
    assert!(
        details.contains("Reason=OutOfMemory"),
        "details:\n{details}"
    );

    let _ = fs::remove_dir_all(&cgroup_base);
}
