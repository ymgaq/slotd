mod helpers;

use helpers::TestRuntime;

#[test]
fn sinfo_filters_partitions_and_supports_custom_formats() {
    let runtime = TestRuntime::with_env(&[
        ("SLOTD_GPU_COUNT", "1"),
        ("SLOTD_CPU_PARTITIONS", "cpu"),
        ("SLOTD_GPU_PARTITIONS", "gpu"),
    ]);

    let all_output = runtime.run_checked(&["sinfo", "--noheader", "-o", "%P %N %t %f %G"]);
    assert!(
        all_output.lines().any(|line| line.contains("cpu")),
        "sinfo:\n{all_output}"
    );
    assert!(
        all_output.lines().any(|line| line.contains("gpu")),
        "sinfo:\n{all_output}"
    );

    let cpu_only = runtime.run_checked(&["sinfo", "-p", "cpu", "--noheader", "-o", "%P"]);
    assert_eq!(
        cpu_only.trim().trim_end_matches('*'),
        "cpu",
        "sinfo:\n{cpu_only}"
    );

    let gpu_only = runtime.run_checked(&["sinfo", "-p", "gpu", "--noheader", "-o", "%P"]);
    assert_eq!(
        gpu_only.trim().trim_end_matches('*'),
        "gpu",
        "sinfo:\n{gpu_only}"
    );
}

#[test]
fn sinfo_long_and_node_views_render_without_headers_when_requested() {
    let runtime = TestRuntime::new();

    let long_output = runtime.run_checked(&["sinfo", "-l"]);
    assert!(
        long_output.contains("PARTITION"),
        "sinfo -l:\n{long_output}"
    );
    assert!(
        long_output.contains("HOSTNAMES"),
        "sinfo -l:\n{long_output}"
    );
    assert!(long_output.contains("CPUS"), "sinfo -l:\n{long_output}");

    let node_view = runtime.run_checked(&["sinfo", "-N", "--noheader", "-o", "%P %N %t"]);
    assert!(
        node_view.lines().all(|line| !line.contains("PARTITION")),
        "sinfo -N --noheader:\n{node_view}"
    );
    assert!(
        node_view
            .lines()
            .next()
            .is_some_and(|line| !line.trim().is_empty()),
        "sinfo -N --noheader:\n{node_view}"
    );
}
