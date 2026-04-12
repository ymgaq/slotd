mod helpers;

use std::fs;

use helpers::TestRuntime;

#[test]
fn srun_cpu_bind_map_cpu_and_cores_apply_affinity() {
    let runtime = TestRuntime::new();
    let total_cpus = std::thread::available_parallelism()
        .expect("available parallelism")
        .get();

    let map_target = total_cpus.saturating_sub(1);
    let map_output = runtime.root_dir().join("cpu-bind-map.out");
    let map_value = format!("map_cpu:{map_target}");
    let result = runtime.run_checked(&[
        "srun",
        "-p",
        "cpu",
        "-c",
        "1",
        "--cpu-bind",
        &map_value,
        "-o",
        map_output.to_str().expect("map output path"),
        "--",
        "bash",
        "-lc",
        "grep '^Cpus_allowed_list:' /proc/self/status | sed 's/.*:[[:space:]]*//'",
    ]);
    assert!(result.is_empty(), "stdout:\n{result}");
    let allowed = fs::read_to_string(&map_output).expect("read map output");
    assert_eq!(allowed.trim(), map_target.to_string());

    if total_cpus >= 2 {
        let cores_output = runtime.root_dir().join("cpu-bind-cores.out");
        let result = runtime.run_checked(&[
            "srun",
            "-p",
            "cpu",
            "-c",
            "2",
            "--cpu-bind",
            "cores",
            "-o",
            cores_output.to_str().expect("cores output path"),
            "--",
            "bash",
            "-lc",
            "grep '^Cpus_allowed_list:' /proc/self/status | sed 's/.*:[[:space:]]*//'",
        ]);
        assert!(result.is_empty(), "stdout:\n{result}");
        let allowed = fs::read_to_string(&cores_output).expect("read cores output");
        let allowed = allowed.trim();
        assert!(
            allowed == "0-1" || allowed == "0,1",
            "unexpected cores affinity: {allowed}"
        );
    }
}

#[test]
fn srun_cpu_bind_rejects_invalid_value() {
    let runtime = TestRuntime::new();

    let output = runtime.run_output(&[
        "srun",
        "-p",
        "cpu",
        "--cpu-bind",
        "bogus",
        "--",
        "true",
    ]);
    assert!(
        !output.status.success(),
        "srun unexpectedly succeeded\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unsupported cpu-bind value: bogus"),
        "stderr:\n{stderr}"
    );
}
