mod helpers;

use std::fs;
use std::process::Stdio;
use std::thread;
use std::time::{Duration, Instant};

use helpers::TestRuntime;

#[test]
fn srun_label_prefixes_stdout_and_stderr_lines() {
    let runtime = TestRuntime::new();

    let output = runtime.run_output(&[
        "srun",
        "-p",
        "cpu",
        "--label",
        "--",
        "bash",
        "-lc",
        "echo labeled-out; echo labeled-err >&2",
    ]);
    assert!(
        output.status.success(),
        "srun failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.contains("0: labeled-out"), "stdout:\n{stdout}");
    assert!(stderr.contains("0: labeled-err"), "stderr:\n{stderr}");
}

#[test]
fn srun_unbuffered_flushes_output_before_process_exit() {
    let runtime = TestRuntime::new();
    let output_path = runtime.root_dir().join("unbuffered.out");

    let mut child = runtime
        .command()
        .args([
            "srun",
            "-p",
            "cpu",
            "--unbuffered",
            "-o",
            output_path.to_str().expect("output path"),
            "--",
            "bash",
            "-lc",
            "printf first; sleep 2; printf second",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn unbuffered srun");

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let current = fs::read_to_string(&output_path).unwrap_or_default();
        if current.contains("first") {
            assert!(
                !current.contains("second"),
                "output was fully buffered unexpectedly: {current}"
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for early unbuffered output"
        );
        thread::sleep(Duration::from_millis(100));
    }

    let status = child.wait().expect("wait for srun");
    assert!(status.success(), "srun exited with status {status}");

    let final_output = fs::read_to_string(&output_path).expect("read unbuffered output");
    assert_eq!(final_output, "firstsecond");
}

#[test]
fn srun_pty_smoke_test_runs_foreground_command() {
    let runtime = TestRuntime::new();

    let output = runtime.run_output(&[
        "srun",
        "-p",
        "cpu",
        "--pty",
        "--",
        "bash",
        "-lc",
        "echo pty-ok",
    ]);
    assert!(
        output.status.success(),
        "srun --pty failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("pty-ok"), "stdout:\n{stdout}");
}
