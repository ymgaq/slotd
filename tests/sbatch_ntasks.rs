mod helpers;

use std::fs;
use std::time::Duration;

use helpers::TestRuntime;

#[test]
fn sbatch_ntasks_launches_one_task_per_rank() {
    let runtime = TestRuntime::new();
    let task_dir = runtime.root_dir().join("sbatch-task-env");
    fs::create_dir_all(&task_dir).expect("create sbatch task dir");

    let job_id = runtime
        .run_checked(&[
            "sbatch",
            "--parsable",
            "-p",
            "cpu",
            "-n",
            "3",
            "-c",
            "1",
            "--wrap",
            &format!(
                "printf '%s|%s|%s|%s' \"$SLURM_JOB_ID\" \"$SLURM_NTASKS\" \"$SLURM_CPUS_PER_TASK\" \"$SLURM_PROCID\" > \"{task_dir}/env-$SLURM_PROCID.txt\"",
                task_dir = task_dir.display(),
            ),
        ])
        .parse::<i64>()
        .expect("parse sbatch job id");

    let state = runtime.wait_for_job_state(job_id, "COMPLETED", Duration::from_secs(10));
    assert_eq!(state, "COMPLETED");

    for procid in 0..3 {
        let env = fs::read_to_string(task_dir.join(format!("env-{procid}.txt")))
            .expect("read sbatch env");
        assert_eq!(env.trim(), format!("{job_id}|3|1|{procid}"));
    }
}
