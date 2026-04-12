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
    let runtime = TestRuntime::with_env(&[
        ("SLOTD_GPU_COUNT", "1"),
        ("SLOTD_CPU_PARTITIONS", "cpu"),
        ("SLOTD_GPU_PARTITIONS", "gpu"),
    ]);

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
    let lines = node_view
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    assert_eq!(lines.len(), 1, "sinfo -N --noheader:\n{node_view}");
    assert!(
        lines[0].contains("cpu") && lines[0].contains("gpu*"),
        "sinfo -N --noheader:\n{node_view}"
    );
    assert!(
        lines[0].contains("localhost") || !lines[0].trim().is_empty(),
        "sinfo -N --noheader:\n{node_view}"
    );
}

#[test]
fn sinfo_supports_named_field_formats() {
    let runtime = TestRuntime::with_env(&[
        ("SLOTD_GPU_COUNT", "1"),
        ("SLOTD_CPU_PARTITIONS", "cpu"),
        ("SLOTD_GPU_PARTITIONS", "gpu"),
    ]);

    let output =
        runtime.run_checked(&["sinfo", "-o", "Partition,Hostnames,State,Features,GresUsed"]);
    assert!(output.contains("PARTITION"), "sinfo:\n{output}");
    assert!(output.contains("HOSTNAMES"), "sinfo:\n{output}");
    assert!(output.contains("FEATURES"), "sinfo:\n{output}");
    assert!(output.contains("GRES_USED"), "sinfo:\n{output}");
    assert!(
        output.lines().any(|line| line.contains("cpu")),
        "sinfo:\n{output}"
    );
}
