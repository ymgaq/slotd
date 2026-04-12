#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub struct TestRuntime {
    tempdir: PathBuf,
    root: PathBuf,
    daemon: Child,
    bin_path: PathBuf,
    daemon_stderr_path: PathBuf,
}

impl TestRuntime {
    pub fn new() -> Self {
        let tempdir = unique_tempdir();
        let root = tempdir.join("slotd-root");
        std::fs::create_dir_all(&root).expect("create slotd root");
        let bin_path = PathBuf::from(env!("CARGO_BIN_EXE_slotd"));
        let daemon_stderr_path = tempdir.join("daemon.stderr.log");
        let daemon_stderr = std::fs::File::create(&daemon_stderr_path).expect("create daemon log");

        let daemon = Command::new(&bin_path)
            .arg("daemon")
            .env("SLOTD_ROOT", &root)
            .env("USER", "slotd-test")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(daemon_stderr))
            .spawn()
            .expect("spawn daemon");

        let mut runtime = Self {
            tempdir,
            root,
            daemon,
            bin_path,
            daemon_stderr_path,
        };
        runtime.wait_for_daemon_ready(Duration::from_secs(10));
        runtime
    }

    pub fn command(&self) -> Command {
        let mut command = Command::new(&self.bin_path);
        command.env("SLOTD_ROOT", &self.root);
        command.env("USER", "slotd-test");
        command.current_dir(self.root_dir());
        command
    }

    pub fn run_checked(&self, args: &[&str]) -> String {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "command failed: {:?}\nstatus: {:?}\nstdout:\n{}\nstderr:\n{}",
            args,
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        String::from_utf8(output.stdout)
            .expect("stdout utf8")
            .trim()
            .to_string()
    }

    pub fn wait_for_job_state(
        &self,
        job_id: i64,
        expected_state: &str,
        timeout: Duration,
    ) -> String {
        let deadline = Instant::now() + timeout;
        loop {
            let state = self.job_state(job_id);
            if state == expected_state {
                return state;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for job {job_id} to reach {expected_state}; last state: {state}"
            );
            thread::sleep(Duration::from_millis(100));
        }
    }

    pub fn wait_for_job_state_in(
        &self,
        job_id: i64,
        expected_states: &[&str],
        timeout: Duration,
    ) -> String {
        let deadline = Instant::now() + timeout;
        loop {
            let state = self.job_state(job_id);
            if expected_states.iter().any(|expected| *expected == state) {
                return state;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for job {job_id} to reach one of {:?}; last state: {state}",
                expected_states
            );
            thread::sleep(Duration::from_millis(100));
        }
    }

    pub fn assert_job_state_stable(&self, job_id: i64, expected_state: &str, duration: Duration) {
        let deadline = Instant::now() + duration;
        loop {
            let state = self.job_state(job_id);
            assert_eq!(
                state, expected_state,
                "job {job_id} changed state during stability check; expected {expected_state}, got {state}"
            );
            if Instant::now() >= deadline {
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    pub fn scontrol_show_job(&self, job_id: i64) -> String {
        self.run_checked(&["scontrol", "show", "job", &job_id.to_string()])
    }

    pub fn root_dir(&self) -> &Path {
        &self.root
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command()
            .args(args)
            .output()
            .unwrap_or_else(|error| panic!("failed to run {:?}: {error}", args))
    }

    fn job_state(&self, job_id: i64) -> String {
        self.run_checked(&[
            "sacct",
            "--parsable2",
            "--noheader",
            "--jobs",
            &job_id.to_string(),
            "--format",
            "State",
        ])
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(str::trim)
        .unwrap_or("")
        .to_string()
    }

    fn wait_for_daemon_ready(&mut self, timeout: Duration) {
        let socket_path = self.root.join("run/slotd.sock");
        let deadline = Instant::now() + timeout;
        loop {
            if socket_path.exists() {
                let output = self.run(&["sinfo", "--noheader"]);
                if output.status.success() {
                    return;
                }
            }

            match self.daemon.try_wait() {
                Ok(Some(status)) => panic!(
                    "daemon exited before becoming ready: {status}\nstderr:\n{}",
                    self.daemon_stderr()
                ),
                Ok(None) => {}
                Err(error) => panic!("failed to check daemon status: {error}"),
            }

            assert!(
                Instant::now() < deadline,
                "timed out waiting for daemon socket at {}",
                socket_path.display()
            );
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn daemon_stderr(&self) -> String {
        std::fs::read_to_string(&self.daemon_stderr_path).unwrap_or_default()
    }
}

impl Drop for TestRuntime {
    fn drop(&mut self) {
        if let Ok(None) = self.daemon.try_wait() {
            let _ = self.daemon.kill();
        }
        let _ = self.daemon.wait();
        let _ = std::fs::remove_dir_all(&self.tempdir);
    }
}

fn unique_tempdir() -> PathBuf {
    let base = std::env::temp_dir();
    for attempt in 0..1000u32 {
        let path = base.join(format!(
            "slotd-test-{}-{}-{attempt}",
            std::process::id(),
            Instant::now().elapsed().as_nanos()
        ));
        match std::fs::create_dir(&path) {
            Ok(()) => return path,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("failed to create tempdir {}: {error}", path.display()),
        }
    }
    panic!("failed to allocate unique tempdir");
}
