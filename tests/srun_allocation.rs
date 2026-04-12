mod helpers;

use std::fs;

use helpers::TestRuntime;

#[test]
fn srun_inside_salloc_creates_step_and_inherits_allocation_env() {
    let runtime = TestRuntime::new();
    let alloc_id_path = runtime.root_dir().join("alloc-job-id.txt");
    let step_env_path = runtime.root_dir().join("step-env.txt");
    let script_path = runtime.root_dir().join("nested-step.sh");

    fs::write(
        &script_path,
        format!(
            r#"#!/usr/bin/env bash
printf '%s' "$SLURM_JOB_ID" > "{alloc_id}"
SLOTD_ROOT="{slotd_root}" USER="slotd-test" "{slotd_bin}" srun bash -lc 'printf "%s|%s" "$SLURM_JOB_ID" "$SLURM_STEP_ID" > "{step_env}"'
"#,
            alloc_id = alloc_id_path.display(),
            slotd_root = runtime.root_dir().display(),
            slotd_bin = env!("CARGO_BIN_EXE_slotd"),
            step_env = step_env_path.display(),
        ),
    )
    .expect("write nested step script");

    let output = runtime.run_output(&[
        "salloc",
        "-p",
        "cpu",
        "bash",
        script_path.to_str().expect("script path"),
    ]);
    assert!(
        output.status.success(),
        "salloc failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let allocation_job_id = stdout
        .lines()
        .find_map(|line| line.strip_prefix("Granted job allocation "))
        .expect("allocation id line")
        .parse::<i64>()
        .expect("parse allocation id");

    let recorded_alloc = fs::read_to_string(&alloc_id_path).expect("read allocation id");
    assert_eq!(recorded_alloc.trim(), allocation_job_id.to_string());

    let step_env = fs::read_to_string(&step_env_path).expect("read step env");
    assert_eq!(step_env.trim(), format!("{allocation_job_id}|0"));

    let sacct = runtime.run_checked(&["sacct", "-P", "-n", "-o", "JobID,State"]);
    assert!(
        sacct.contains(&format!("{allocation_job_id}|")),
        "sacct:\n{sacct}"
    );
    assert!(
        sacct.contains(&format!("{allocation_job_id}.0|COMPLETED")),
        "sacct:\n{sacct}"
    );
}
