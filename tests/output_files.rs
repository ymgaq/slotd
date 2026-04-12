mod helpers;

use std::fs;
use std::time::Duration;

use helpers::TestRuntime;

#[test]
fn sbatch_writes_default_stdout_file() {
    let runtime = TestRuntime::new();

    let job_id = runtime
        .run_checked(&["sbatch", "--parsable", "--wrap", "echo hello-output"])
        .parse::<i64>()
        .expect("parse job id");

    let final_state = runtime.wait_for_job_state(job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(final_state, "COMPLETED");

    let stdout_path = runtime.root_dir().join(format!("slurm-{job_id}.out"));
    let stdout = fs::read_to_string(&stdout_path).expect("read default stdout");
    assert!(stdout.contains("hello-output"), "stdout:\n{stdout}");
}

#[test]
fn srun_foreground_honors_output_and_error_paths() {
    let runtime = TestRuntime::new();
    let stdout_path = runtime.root_dir().join("srun.out");
    let stderr_path = runtime.root_dir().join("srun.err");

    let output = runtime.run_checked(&[
        "srun",
        "-o",
        stdout_path.to_str().expect("stdout path"),
        "-e",
        stderr_path.to_str().expect("stderr path"),
        "--",
        "bash",
        "-lc",
        "echo foreground-out; echo foreground-err >&2",
    ]);
    assert!(output.is_empty(), "stdout:\n{output}");

    let stdout = fs::read_to_string(&stdout_path).expect("read srun stdout");
    let stderr = fs::read_to_string(&stderr_path).expect("read srun stderr");
    assert!(stdout.contains("foreground-out"), "stdout file:\n{stdout}");
    assert!(stderr.contains("foreground-err"), "stderr file:\n{stderr}");
}
