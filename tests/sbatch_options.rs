mod helpers;

use std::fs;
use std::time::Duration;

use helpers::TestRuntime;

#[test]
fn sbatch_export_and_export_file_propagate_environment() {
    let runtime = TestRuntime::new();
    let export_file = runtime.root_dir().join("env.list");
    fs::write(&export_file, "FROM_FILE=via-file\nBASE_ONLY=seed\n").expect("write export file");

    let job_id = runtime
        .command()
        .env("CLI_ONLY", "via-cli")
        .args([
            "sbatch",
            "--parsable",
            "--partition",
            "cpu",
            "--export-file",
            export_file.to_str().expect("export-file path"),
            "--export",
            "CLI_ONLY,OVERRIDE=explicit",
            "--wrap",
            "printf '%s|%s|%s' \"$CLI_ONLY\" \"$FROM_FILE\" \"$OVERRIDE\"",
        ])
        .output()
        .expect("run sbatch");
    assert!(
        job_id.status.success(),
        "sbatch failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&job_id.stdout),
        String::from_utf8_lossy(&job_id.stderr),
    );
    let job_id = String::from_utf8(job_id.stdout)
        .expect("job id utf8")
        .trim()
        .parse::<i64>()
        .expect("parse job id");

    let final_state = runtime.wait_for_job_state(job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(final_state, "COMPLETED");

    let stdout = fs::read_to_string(runtime.root_dir().join(format!("slurm-{job_id}.out")))
        .expect("read stdout");
    assert_eq!(stdout.trim(), "via-cli|via-file|explicit");
}

#[test]
fn sbatch_open_mode_append_and_truncate_affect_output_file() {
    let runtime = TestRuntime::new();
    let output_path = runtime.root_dir().join("shared.out");

    for message in ["first", "second"] {
        let output = runtime.run_checked(&[
            "sbatch",
            "--parsable",
            "--partition",
            "cpu",
            "--open-mode",
            "append",
            "-o",
            output_path.to_str().expect("output path"),
            "--wrap",
            &format!("echo {message}"),
        ]);
        let job_id = output.parse::<i64>().expect("parse append job id");
        let final_state = runtime.wait_for_job_state(job_id, "COMPLETED", Duration::from_secs(10));
        assert_eq!(final_state, "COMPLETED");
    }

    let appended = fs::read_to_string(&output_path).expect("read appended output");
    assert!(appended.contains("first"), "output:\n{appended}");
    assert!(appended.contains("second"), "output:\n{appended}");

    let output = runtime.run_checked(&[
        "sbatch",
        "--parsable",
        "--partition",
        "cpu",
        "--open-mode",
        "truncate",
        "-o",
        output_path.to_str().expect("output path"),
        "--wrap",
        "echo replacement",
    ]);
    let job_id = output.parse::<i64>().expect("parse truncate job id");
    let final_state = runtime.wait_for_job_state(job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(final_state, "COMPLETED");

    let truncated = fs::read_to_string(&output_path).expect("read truncated output");
    assert_eq!(truncated.trim(), "replacement");
}

#[test]
fn sbatch_chdir_sets_workdir_and_relative_output_root() {
    let runtime = TestRuntime::new();
    let workdir = runtime.root_dir().join("nested").join("work");
    fs::create_dir_all(&workdir).expect("create workdir");

    let output = runtime.run_checked(&[
        "sbatch",
        "--parsable",
        "--partition",
        "cpu",
        "-D",
        workdir.to_str().expect("workdir path"),
        "--wrap",
        "pwd",
    ]);
    let job_id = output.parse::<i64>().expect("parse chdir job id");
    let final_state = runtime.wait_for_job_state(job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(final_state, "COMPLETED");

    let details = runtime.scontrol_show_job(job_id);
    assert!(
        details.contains(&format!("WorkDir={}", workdir.display())),
        "details:\n{details}"
    );

    let stdout = fs::read_to_string(workdir.join(format!("slurm-{job_id}.out"))).expect("read chdir stdout");
    assert_eq!(stdout.trim(), workdir.display().to_string());
}
