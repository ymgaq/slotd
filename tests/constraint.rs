mod helpers;

use helpers::TestRuntime;

#[test]
fn sbatch_rejects_unknown_constraint() {
    let runtime = TestRuntime::new();

    let output = runtime.run_output(&[
        "sbatch",
        "--constraint",
        "missing-feature",
        "--wrap",
        "true",
    ]);
    assert!(
        !output.status.success(),
        "sbatch unexpectedly succeeded\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("constraint \"missing-feature\" does not match local features"),
        "stderr:\n{stderr}"
    );
}
