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

    let job_cgroup = format!("{cgroup_base}/slotd-{job_id}");
    runtime.wait_for_condition(Duration::from_secs(5), || {
        std::path::Path::new(&job_cgroup)
            .join("memory.max")
            .exists()
            && std::path::Path::new(&job_cgroup).join("cpu.max").exists()
    });

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

#[test]
fn sbatch_without_cgroup_base_stays_reservation_only() {
    let runtime = TestRuntime::new();
    let fake_base = unique_fake_cgroup_base();

    let job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "--mem",
            "64M",
            "--wrap",
            &format!(
                "if [ -d \"{0}/slotd-$SLURM_JOB_ID\" ]; then printf \"oom_kill 1\\n\" > \"{0}/slotd-$SLURM_JOB_ID/memory.events\"; fi",
                fake_base
            ),
        ])
        .parse::<i64>()
        .expect("parse job id");

    let final_state = runtime.wait_for_job_state(job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(final_state, "COMPLETED");
    assert!(
        !std::path::Path::new(&format!("{fake_base}/slotd-{job_id}")).exists(),
        "unexpected cgroup directory created without SLOTD_CGROUP_BASE"
    );
}

#[test]
fn sbatch_fails_clearly_when_cgroup_base_is_invalid() {
    let invalid_base = std::env::temp_dir().join(format!(
        "slotd-invalid-cgroup-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::write(&invalid_base, "not a directory").expect("write invalid cgroup base marker");
    let runtime = TestRuntime::with_env(&[(
        "SLOTD_CGROUP_BASE",
        invalid_base.to_str().expect("invalid cgroup path"),
    )]);

    let output = runtime.run_output(&["sbatch", "--parsable", "--wrap", "true"]);
    assert!(
        !output.status.success(),
        "sbatch unexpectedly succeeded\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("failed to apply cgroup controls under SLOTD_CGROUP_BASE"),
        "stderr:\n{stderr}"
    );

    let _ = fs::remove_file(&invalid_base);
}
