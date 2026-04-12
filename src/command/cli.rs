use std::ffi::OsString;
use std::path::PathBuf;

use crate::app::config::AppConfig;
use crate::app::error::Result;
use crate::command::args::Commands;
use crate::command::handlers;
use crate::command::submit::{run_salloc, run_sbatch, run_srun};
use crate::runtime::daemon;
#[cfg(test)]
use crate::util::time::{format_duration_secs, format_timestamp};

pub use crate::command::args::Cli;

impl Cli {
    pub fn run(self) -> Result<()> {
        let config = AppConfig::load();
        match self.command {
            Commands::Daemon => daemon::run(config),
            Commands::Sbatch(args) => run_sbatch(config, args),
            Commands::Srun(args) => run_srun(config, args),
            Commands::Salloc(args) => run_salloc(config, args),
            Commands::Scontrol(args) => handlers::run_scontrol(config, args),
            Commands::Squeue(args) => handlers::run_squeue(config, args),
            Commands::Sacct(args) => handlers::run_sacct(config, args),
            Commands::Scancel(args) => handlers::run_scancel(config, args),
            Commands::Sinfo(args) => handlers::run_sinfo(config, args),
        }
    }
}

pub fn dispatch_argv0(mut argv: Vec<OsString>) -> Vec<OsString> {
    let Some(first) = argv.first() else {
        return argv;
    };

    let command = PathBuf::from(first)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("slotd")
        .to_string();

    let alias = match command.as_str() {
        "sbatch" => Some("sbatch"),
        "srun" => Some("srun"),
        "salloc" => Some("salloc"),
        "scontrol" => Some("scontrol"),
        "squeue" => Some("squeue"),
        "sacct" => Some("sacct"),
        "scancel" => Some("scancel"),
        "sinfo" => Some("sinfo"),
        _ => None,
    };

    if let Some(alias) = alias {
        argv.insert(1, OsString::from(alias));
    }
    argv
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use clap::CommandFactory;

    use super::{dispatch_argv0, format_duration_secs, format_timestamp};
    use crate::app::config::AppConfig;
    use crate::command::args::{
        CORE_RESOURCE_LONG_FLAGS, Cli, ResourceArgs, SUPPORTED_ROOT_COMMANDS,
        SUPPORTED_USER_COMMANDS,
    };
    use crate::command::helpers::{
        estimate_start_times, load_sbatch_env_overrides_with, merge_batch_directives,
    };
    use crate::model::job::{JobRecord, JobState, OpenMode};
    use crate::runtime::cpu::resolve_cpu_bind_ids;
    use crate::submit::sbatch::BatchDirectives;
    use crate::util::env::resolve_export_spec;
    use crate::util::signals::parse_signal_name;
    use crate::util::signals::parse_warning_signal;
    use crate::util::time::now_ts;
    use crate::util::time::parse_begin_time;
    use crate::util::time::parse_time_filter;

    #[test]
    fn argv0_dispatch_inserts_slurm_alias() {
        let argv = vec![OsString::from("squeue"), OsString::from("--noheader")];
        let dispatched = dispatch_argv0(argv);
        assert_eq!(dispatched[1], OsString::from("squeue"));
        assert_eq!(dispatched[2], OsString::from("--noheader"));
    }

    #[test]
    fn phase0_supported_commands_are_explicitly_fixed() {
        let command = Cli::command();
        let names = command
            .get_subcommands()
            .map(|subcommand| subcommand.get_name())
            .collect::<Vec<_>>();
        assert_eq!(names, SUPPORTED_ROOT_COMMANDS);

        let user_commands = names
            .iter()
            .copied()
            .filter(|name| *name != "daemon")
            .collect::<Vec<_>>();
        assert_eq!(user_commands, SUPPORTED_USER_COMMANDS);
    }

    #[test]
    fn phase0_core_resource_flags_are_shared_across_submission_commands() {
        let command = Cli::command();
        for subcommand_name in ["sbatch", "srun", "salloc"] {
            let subcommand = command
                .get_subcommands()
                .find(|subcommand| subcommand.get_name() == subcommand_name)
                .expect("subcommand exists");
            let option_names = subcommand
                .get_arguments()
                .filter_map(|argument| argument.get_long())
                .collect::<Vec<_>>();
            for flag in CORE_RESOURCE_LONG_FLAGS {
                assert!(
                    option_names.contains(flag),
                    "{subcommand_name} is missing shared resource flag --{flag}"
                );
            }
        }
    }

    #[test]
    fn phase0_resource_model_uses_one_shared_defaulting_path() {
        let config = AppConfig::load();
        let args = ResourceArgs {
            job_name: Some("demo".to_string()),
            partition: Some(config.default_partition().to_string()),
            cpus_per_task: Some(4),
            ntasks: Some(2),
            mem: Some("2G".to_string()),
            time: Some("00:30:00".to_string()),
            gpus: Some(0),
            chdir: Some("/tmp".into()),
            constraint: None,
        };

        let resolved = args.resolve(&config, None).expect("resource args resolve");
        assert_eq!(resolved.job_name.as_deref(), Some("demo"));
        assert_eq!(resolved.partition, config.default_partition());
        assert_eq!(resolved.cwd, "/tmp");
        assert_eq!(resolved.requested_cpus, 4);
        assert_eq!(resolved.requested_tasks, 2);
        assert_eq!(resolved.requested_memory_mb, 2048);
        assert_eq!(resolved.requested_gpus, 0);
        assert_eq!(resolved.time_limit_secs, Some(1800));
    }

    #[test]
    fn phase1_cli_resource_values_override_batch_directives() {
        let config = AppConfig::load();
        let directives = BatchDirectives {
            job_name: Some("from-directive".to_string()),
            partition: Some(config.default_partition().to_string()),
            cpus_per_task: Some(2),
            ntasks: Some(3),
            mem_mb: Some(1024),
            gpus: Some(1),
            constraint: None,
            time_limit_secs: Some(600),
            begin: None,
            exclusive: false,
            requeue: false,
            dependency: None,
            array_spec: None,
            output_path: None,
            error_path: None,
            chdir: Some("/directive".to_string()),
        };
        let args = ResourceArgs {
            job_name: Some("from-cli".to_string()),
            partition: Some(config.default_partition().to_string()),
            cpus_per_task: Some(4),
            ntasks: Some(5),
            mem: Some("2G".to_string()),
            time: Some("00:30:00".to_string()),
            gpus: Some(0),
            chdir: Some("/cli".into()),
            constraint: None,
        };

        let resolved = args
            .resolve(&config, Some(&directives))
            .expect("resource args resolve");
        assert_eq!(resolved.job_name.as_deref(), Some("from-cli"));
        assert_eq!(resolved.cwd, "/cli");
        assert_eq!(resolved.requested_cpus, 4);
        assert_eq!(resolved.requested_tasks, 5);
        assert_eq!(resolved.requested_memory_mb, 2048);
        assert_eq!(resolved.requested_gpus, 0);
        assert_eq!(resolved.time_limit_secs, Some(1800));
    }

    #[test]
    fn phase15_sbatch_environment_overrides_directives() {
        let env = load_sbatch_env_overrides_with(|name| match name {
            "SBATCH_PARTITION" => Some("cpu".to_string()),
            "SBATCH_CPUS_PER_TASK" => Some("8".to_string()),
            "SBATCH_TIME" => Some("01:00:00".to_string()),
            "SBATCH_OUTPUT" => Some("from-env.out".to_string()),
            _ => None,
        });
        let directives = BatchDirectives {
            partition: Some("gpu".to_string()),
            cpus_per_task: Some(2),
            time_limit_secs: Some(300),
            output_path: Some("from-directive.out".to_string()),
            ..BatchDirectives::default()
        };
        let merged = merge_batch_directives(&directives, &env.directives);
        assert_eq!(merged.partition.as_deref(), Some("cpu"));
        assert_eq!(merged.cpus_per_task, Some(8));
        assert_eq!(merged.time_limit_secs, Some(3600));
        assert_eq!(merged.output_path.as_deref(), Some("from-env.out"));
    }

    #[test]
    fn phase15_sbatch_environment_keeps_non_overridden_directives() {
        let env = load_sbatch_env_overrides_with(|_| None);
        let directives = BatchDirectives {
            partition: Some("cpu".to_string()),
            cpus_per_task: Some(2),
            output_path: Some("from-directive.out".to_string()),
            ..BatchDirectives::default()
        };
        let merged = merge_batch_directives(&directives, &env.directives);
        assert_eq!(merged.partition.as_deref(), Some("cpu"));
        assert_eq!(merged.cpus_per_task, Some(2));
        assert_eq!(merged.output_path.as_deref(), Some("from-directive.out"));
    }

    #[test]
    fn phase5_sbatch_requeue_environment_overrides_directives() {
        let env = load_sbatch_env_overrides_with(|name| match name {
            "SBATCH_REQUEUE" => Some("yes".to_string()),
            _ => None,
        });
        let directives = BatchDirectives {
            requeue: false,
            ..BatchDirectives::default()
        };
        let merged = merge_batch_directives(&directives, &env.directives);
        assert!(merged.requeue);
    }

    #[test]
    fn phase3_constraint_is_shared_and_validated() {
        let config = AppConfig::load();
        let args = ResourceArgs {
            job_name: None,
            partition: Some(config.default_partition().to_string()),
            cpus_per_task: None,
            ntasks: None,
            mem: None,
            time: None,
            gpus: None,
            chdir: None,
            constraint: Some("cpu".to_string()),
        };
        let resolved = args.resolve(&config, None).expect("resource args resolve");
        assert_eq!(resolved.constraint.as_deref(), Some("cpu"));
    }

    #[test]
    fn phase3_cpu_bind_map_cpu_is_parsed() {
        let cpu_ids = resolve_cpu_bind_ids(Some("map_cpu:0,2,2"), 8, 4)
            .expect("cpu bind")
            .expect("cpu ids");
        assert_eq!(cpu_ids, vec![0, 2]);
    }

    #[test]
    fn phase4_begin_time_supports_now_offset() {
        let before = now_ts();
        let begin = parse_begin_time("now+00:10:00").expect("begin time");
        let after = now_ts();
        assert!(begin >= before + 600);
        assert!(begin <= after + 600);
    }

    #[test]
    fn parses_date_only_time_filter() {
        assert_eq!(parse_time_filter("1970-01-02").expect("parse date"), 86_400);
    }

    #[test]
    fn parses_full_timestamp_time_filter() {
        assert_eq!(
            parse_time_filter("1970-01-02T03:04:05").expect("parse datetime"),
            97_445
        );
    }

    #[test]
    fn formats_timestamp_and_duration() {
        assert_eq!(format_timestamp(97_445), "1970-01-02T03:04:05");
        assert_eq!(format_duration_secs(3_661), "01:01:01");
    }

    #[test]
    fn phase2_export_spec_none_clears_seed_values() {
        let resolved = resolve_export_spec("NONE", &[("KEEP".to_string(), "value".to_string())])
            .expect("resolve export");
        assert!(resolved.is_empty());
    }

    #[test]
    fn phase2_export_spec_updates_seed_and_adds_assignments() {
        let resolved = resolve_export_spec(
            "FOO=updated,BAR=baz",
            &[("FOO".to_string(), "old".to_string())],
        )
        .expect("resolve export");
        assert_eq!(
            resolved,
            vec![
                ("FOO".to_string(), "updated".to_string()),
                ("BAR".to_string(), "baz".to_string()),
            ]
        );
    }

    #[test]
    fn phase2_warning_signal_parses_batch_prefix_and_offset() {
        let warning = parse_warning_signal("B:USR1@90").expect("warning signal");
        assert_eq!(warning.signal, parse_signal_name("USR1").expect("signal"));
        assert_eq!(warning.seconds_before_end, 90);
    }

    #[test]
    fn phase2_warning_signal_defaults_offset_to_sixty_seconds() {
        let warning = parse_warning_signal("TERM").expect("warning signal");
        assert_eq!(warning.signal, parse_signal_name("TERM").expect("signal"));
        assert_eq!(warning.seconds_before_end, 60);
    }

    #[test]
    fn phase2_signal_parser_accepts_signal_names_and_numbers() {
        assert_eq!(
            parse_signal_name("SIGTERM").expect("named signal"),
            parse_signal_name("TERM").expect("canonical signal")
        );
        assert_eq!(parse_signal_name("15").expect("numeric signal"), 15);
    }

    #[test]
    fn phase2_start_time_estimator_marks_jobs_that_fit_now() {
        let mut config = AppConfig::load();
        config.total_cpus = 8;
        config.total_memory_mb = 16_384;
        config.total_gpus = 1;
        let jobs = vec![JobRecord {
            id: 42,
            parent_job_id: None,
            step_id: None,
            held: false,
            priority: 0,
            array_job_id: None,
            array_task_id: None,
            array_task_count: None,
            array_task_limit: None,
            user_name: "user".to_string(),
            partition: config.default_partition().to_string(),
            name: "pending".to_string(),
            state: JobState::Pending,
            command: "sleep 1".to_string(),
            exit_code: None,
            allocation_only: false,
            dependency: None,
            max_rss_kb: None,
            submit_time: 0,
            start_time: None,
            end_time: None,
            pid: None,
            pgid: None,
            requested_cpus: 2,
            requested_tasks: 1,
            requested_memory_mb: 512,
            requested_gpus: 0,
            assigned_gpu_ids: Vec::new(),
            cwd: "/tmp".to_string(),
            script_path: String::new(),
            stdout_path: "slurm-42.out".to_string(),
            stderr_path: "slurm-42.out".to_string(),
            constraint: None,
            cpu_bind: None,
            state_reason: None,
            term_signal: None,
            time_limit_secs: Some(300),
            begin_time: None,
            exclusive: false,
            export_env: Vec::new(),
            open_mode: OpenMode::Truncate,
            warning_signal: None,
            requeue: false,
            requeue_count: 0,
        }];

        let start_times = estimate_start_times(&config, &jobs);
        let start = start_times.get(&42).expect("start time");
        assert_ne!(start, "N/A");
    }
}
