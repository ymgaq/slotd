use std::path::{Path, PathBuf};

use crate::error::{Result, SlotdError};

#[derive(Debug, Default, Clone)]
pub struct BatchDirectives {
    pub job_name: Option<String>,
    pub partition: Option<String>,
    pub cpus_per_task: Option<u32>,
    pub ntasks: Option<u32>,
    pub mem_mb: Option<u64>,
    pub gpus: Option<u32>,
    pub constraint: Option<String>,
    pub time_limit_secs: Option<u64>,
    pub dependency: Option<String>,
    pub array_spec: Option<String>,
    pub output_path: Option<String>,
    pub error_path: Option<String>,
    pub chdir: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArraySpec {
    pub task_ids: Vec<i32>,
    pub limit: Option<u32>,
}

pub fn parse_directives(script_body: &str) -> Result<BatchDirectives> {
    let mut directives = BatchDirectives::default();

    for raw_line in script_body.lines() {
        let trimmed = raw_line.trim();
        if trimmed.starts_with("#!") {
            continue;
        }
        if !trimmed.starts_with("#SBATCH") {
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            break;
        }

        let args = trimmed.trim_start_matches("#SBATCH").trim();
        if args.is_empty() {
            continue;
        }

        let tokens = split_tokens(args);
        apply_tokens(&mut directives, &tokens)?;
    }

    Ok(directives)
}

pub fn resolve_log_path(cwd: &str, path: &str) -> String {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        return path.to_string_lossy().to_string();
    }

    Path::new(cwd).join(path).to_string_lossy().to_string()
}

pub fn expand_output_pattern(
    pattern: &str,
    job_id: i64,
    job_name: &str,
    user_name: &str,
    hostname: &str,
    array_job_id: Option<i64>,
    array_task_id: Option<i32>,
) -> String {
    let mut output = String::new();
    let mut chars = pattern.chars();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            output.push(ch);
            continue;
        }

        match chars.next() {
            Some('%') => output.push('%'),
            Some('j') => output.push_str(&job_id.to_string()),
            Some('A') => output.push_str(&array_job_id.unwrap_or(job_id).to_string()),
            Some('a') => output.push_str(
                &array_task_id
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "4294967294".to_string()),
            ),
            Some('x') => output.push_str(job_name),
            Some('u') => output.push_str(user_name),
            Some('N') => output.push_str(hostname),
            Some(other) => {
                output.push('%');
                output.push(other);
            }
            None => output.push('%'),
        }
    }
    output
}

pub fn default_batch_output_pattern() -> &'static str {
    "slurm-%j.out"
}

pub fn parse_array_spec(value: &str) -> Result<ArraySpec> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(SlotdError::from("array specification cannot be empty"));
    }

    let (range_expr, limit) = match trimmed.split_once('%') {
        Some((expr, limit)) => (
            expr.trim(),
            Some(
                limit
                    .trim()
                    .parse::<u32>()
                    .map_err(|_| SlotdError::from(format!("invalid array limit: {trimmed}")))?,
            ),
        ),
        None => (trimmed, None),
    };

    let mut task_ids = Vec::new();
    for segment in range_expr.split(',') {
        parse_array_segment(segment.trim(), &mut task_ids, trimmed)?;
    }
    task_ids.sort_unstable();
    task_ids.dedup();

    if task_ids.is_empty() {
        return Err(SlotdError::from(format!(
            "array specification produced no tasks: {trimmed}"
        )));
    }

    Ok(ArraySpec { task_ids, limit })
}

pub fn parse_mem_mb(value: &str) -> Result<u64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(SlotdError::from("memory value cannot be empty"));
    }

    let split_at = trimmed
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(trimmed.len());
    let (number_part, unit_part) = trimmed.split_at(split_at);
    let base: u64 = number_part
        .parse()
        .map_err(|_| SlotdError::from(format!("invalid memory value: {trimmed}")))?;

    let normalized = unit_part.trim().to_ascii_uppercase();
    let mem_mb = match normalized.as_str() {
        "" | "M" => base,
        "K" => base.div_ceil(1024),
        "G" => base.saturating_mul(1024),
        "T" => base.saturating_mul(1024 * 1024),
        _ => {
            return Err(SlotdError::from(format!(
                "unsupported memory unit in value: {trimmed}"
            )));
        }
    };

    Ok(mem_mb)
}

pub fn parse_time_limit_secs(value: &str) -> Result<u64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(SlotdError::from("time limit cannot be empty"));
    }

    let (days, rest) = if let Some((days, rest)) = trimmed.split_once('-') {
        let days: u64 = days
            .parse()
            .map_err(|_| SlotdError::from(format!("invalid time limit: {trimmed}")))?;
        (days, rest)
    } else {
        (0, trimmed)
    };

    let parts = rest.split(':').collect::<Vec<_>>();
    let (hours, minutes, seconds) = match parts.as_slice() {
        [m] => (0, parse_time_component(m, trimmed)?, 0),
        [m, s] => (
            0,
            parse_time_component(m, trimmed)?,
            parse_time_component(s, trimmed)?,
        ),
        [h, m, s] => (
            parse_time_component(h, trimmed)?,
            parse_time_component(m, trimmed)?,
            parse_time_component(s, trimmed)?,
        ),
        _ => return Err(SlotdError::from(format!("invalid time limit: {trimmed}"))),
    };

    Ok(days * 86_400 + hours * 3_600 + minutes * 60 + seconds)
}

fn apply_tokens(directives: &mut BatchDirectives, tokens: &[String]) -> Result<()> {
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        let next = tokens.get(index + 1).map(String::as_str);

        let consumed = if let Some(value) = token.strip_prefix("--job-name=") {
            directives.job_name = Some(value.to_string());
            1
        } else if let Some(value) = token.strip_prefix("-J=") {
            directives.job_name = Some(value.to_string());
            1
        } else if let Some(value) = token.strip_prefix("--partition=") {
            directives.partition = Some(value.to_string());
            1
        } else if let Some(value) = token.strip_prefix("-p=") {
            directives.partition = Some(value.to_string());
            1
        } else if token == "--partition" || token == "-p" {
            directives.partition = Some(require_value(token, next)?.to_string());
            2
        } else if token == "--job-name" || token == "-J" {
            directives.job_name = Some(require_value(token, next)?.to_string());
            2
        } else if let Some(value) = token.strip_prefix("--cpus-per-task=") {
            directives.cpus_per_task = Some(parse_u32("--cpus-per-task", value)?);
            1
        } else if let Some(value) = token.strip_prefix("-c=") {
            directives.cpus_per_task = Some(parse_u32("-c", value)?);
            1
        } else if token == "--cpus-per-task" || token == "-c" {
            directives.cpus_per_task = Some(parse_u32(token, require_value(token, next)?)?);
            2
        } else if let Some(value) = token.strip_prefix("--ntasks=") {
            directives.ntasks = Some(parse_u32("--ntasks", value)?);
            1
        } else if let Some(value) = token.strip_prefix("-n=") {
            directives.ntasks = Some(parse_u32("-n", value)?);
            1
        } else if token == "--ntasks" || token == "-n" {
            directives.ntasks = Some(parse_u32(token, require_value(token, next)?)?);
            2
        } else if let Some(value) = token.strip_prefix("--mem=") {
            directives.mem_mb = Some(parse_mem_mb(value)?);
            1
        } else if token == "--mem" {
            directives.mem_mb = Some(parse_mem_mb(require_value(token, next)?)?);
            2
        } else if let Some(value) = token.strip_prefix("--gpus=") {
            directives.gpus = Some(parse_u32("--gpus", value)?);
            1
        } else if let Some(value) = token.strip_prefix("--constraint=") {
            directives.constraint = Some(value.to_string());
            1
        } else if token == "--constraint" {
            directives.constraint = Some(require_value(token, next)?.to_string());
            2
        } else if let Some(value) = token.strip_prefix("--time=") {
            directives.time_limit_secs = Some(parse_time_limit_secs(value)?);
            1
        } else if let Some(value) = token.strip_prefix("-t=") {
            directives.time_limit_secs = Some(parse_time_limit_secs(value)?);
            1
        } else if token == "--time" || token == "-t" {
            directives.time_limit_secs = Some(parse_time_limit_secs(require_value(token, next)?)?);
            2
        } else if let Some(value) = token.strip_prefix("--dependency=") {
            directives.dependency = Some(value.to_string());
            1
        } else if let Some(value) = token.strip_prefix("-d=") {
            directives.dependency = Some(value.to_string());
            1
        } else if token == "--dependency" || token == "-d" {
            directives.dependency = Some(require_value(token, next)?.to_string());
            2
        } else if let Some(value) = token.strip_prefix("--array=") {
            directives.array_spec = Some(value.to_string());
            1
        } else if let Some(value) = token.strip_prefix("-a=") {
            directives.array_spec = Some(value.to_string());
            1
        } else if token == "--array" || token == "-a" {
            directives.array_spec = Some(require_value(token, next)?.to_string());
            2
        } else if let Some(value) = token.strip_prefix("-G=") {
            directives.gpus = Some(parse_u32("-G", value)?);
            1
        } else if token == "--gpus" || token == "-G" {
            directives.gpus = Some(parse_u32(token, require_value(token, next)?)?);
            2
        } else if let Some(value) = token.strip_prefix("--output=") {
            directives.output_path = Some(value.to_string());
            1
        } else if let Some(value) = token.strip_prefix("-o=") {
            directives.output_path = Some(value.to_string());
            1
        } else if token == "--output" || token == "-o" {
            directives.output_path = Some(require_value(token, next)?.to_string());
            2
        } else if let Some(value) = token.strip_prefix("--error=") {
            directives.error_path = Some(value.to_string());
            1
        } else if let Some(value) = token.strip_prefix("-e=") {
            directives.error_path = Some(value.to_string());
            1
        } else if token == "--error" || token == "-e" {
            directives.error_path = Some(require_value(token, next)?.to_string());
            2
        } else if let Some(value) = token.strip_prefix("--chdir=") {
            directives.chdir = Some(value.to_string());
            1
        } else if let Some(value) = token.strip_prefix("-D=") {
            directives.chdir = Some(value.to_string());
            1
        } else if token == "--chdir" || token == "-D" {
            directives.chdir = Some(require_value(token, next)?.to_string());
            2
        } else {
            1
        };

        index += consumed;
    }

    Ok(())
}

fn require_value<'a>(flag: &str, value: Option<&'a str>) -> Result<&'a str> {
    value.ok_or_else(|| SlotdError::from(format!("missing value for {flag}")))
}

fn parse_u32(flag: &str, value: &str) -> Result<u32> {
    value
        .parse()
        .map_err(|_| SlotdError::from(format!("invalid value for {flag}: {value}")))
}

fn parse_time_component(value: &str, original: &str) -> Result<u64> {
    value
        .parse()
        .map_err(|_| SlotdError::from(format!("invalid time limit: {original}")))
}

fn parse_array_segment(segment: &str, task_ids: &mut Vec<i32>, original: &str) -> Result<()> {
    if segment.is_empty() {
        return Err(SlotdError::from(format!(
            "invalid array specification: {original}"
        )));
    }

    let (range_part, step) = match segment.split_once(':') {
        Some((range, step)) => (
            range.trim(),
            step.trim()
                .parse::<i32>()
                .map_err(|_| SlotdError::from(format!("invalid array step: {original}")))?,
        ),
        None => (segment, 1),
    };

    if step <= 0 {
        return Err(SlotdError::from(format!("invalid array step: {original}")));
    }

    if let Some((start, end)) = range_part.split_once('-') {
        let start = parse_array_task_id(start.trim(), original)?;
        let end = parse_array_task_id(end.trim(), original)?;
        if start > end {
            return Err(SlotdError::from(format!("invalid array range: {original}")));
        }
        let mut current = start;
        while current <= end {
            task_ids.push(current);
            current += step;
        }
    } else {
        task_ids.push(parse_array_task_id(range_part.trim(), original)?);
    }

    Ok(())
}

fn parse_array_task_id(value: &str, original: &str) -> Result<i32> {
    value
        .parse::<i32>()
        .map_err(|_| SlotdError::from(format!("invalid array task id in: {original}")))
}

fn split_tokens(input: &str) -> Vec<String> {
    input.split_whitespace().map(ToString::to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        ArraySpec, default_batch_output_pattern, expand_output_pattern, parse_array_spec,
        parse_directives, parse_time_limit_secs,
    };

    #[test]
    fn parses_long_and_short_sbatch_directives() {
        let script = "\
#!/bin/bash
#SBATCH -J demo
#SBATCH -p gpu
#SBATCH -c 4
#SBATCH -n 3
#SBATCH -d afterok:10
#SBATCH -a 0-3%2
#SBATCH -G 2
#SBATCH -o logs/out.txt
#SBATCH -e logs/err.txt
#SBATCH -D /tmp/work
echo hi
";
        let directives = parse_directives(script).expect("parse directives");
        assert_eq!(directives.job_name.as_deref(), Some("demo"));
        assert_eq!(directives.partition.as_deref(), Some("gpu"));
        assert_eq!(directives.cpus_per_task, Some(4));
        assert_eq!(directives.ntasks, Some(3));
        assert_eq!(directives.dependency.as_deref(), Some("afterok:10"));
        assert_eq!(directives.array_spec.as_deref(), Some("0-3%2"));
        assert_eq!(directives.gpus, Some(2));
        assert_eq!(directives.output_path.as_deref(), Some("logs/out.txt"));
        assert_eq!(directives.error_path.as_deref(), Some("logs/err.txt"));
        assert_eq!(directives.chdir.as_deref(), Some("/tmp/work"));
    }

    #[test]
    fn ignores_directives_after_first_executable_line() {
        let script = "\
#!/bin/bash
#SBATCH -J before
echo start
#SBATCH -J after
";
        let directives = parse_directives(script).expect("parse directives");
        assert_eq!(directives.job_name.as_deref(), Some("before"));
    }

    #[test]
    fn expands_common_output_pattern_tokens() {
        let value = expand_output_pattern(
            "logs/%x-%j-%A-%a-%%-%u-%N.out",
            42,
            "demo",
            "alice",
            "node1",
            Some(12),
            Some(3),
        );
        assert_eq!(value, "logs/demo-42-12-3-%-alice-node1.out");
        assert_eq!(default_batch_output_pattern(), "slurm-%j.out");
    }

    #[test]
    fn parses_time_limit_variants() {
        assert_eq!(parse_time_limit_secs("90").expect("minutes"), 5_400);
        assert_eq!(parse_time_limit_secs("01:30:00").expect("hms"), 5_400);
        assert_eq!(parse_time_limit_secs("1-00:00:00").expect("days"), 86_400);
    }

    #[test]
    fn parses_array_ranges_steps_and_limit() {
        let spec = parse_array_spec("0-6:2,9%3").expect("array spec");
        assert_eq!(
            spec,
            ArraySpec {
                task_ids: vec![0, 2, 4, 6, 9],
                limit: Some(3)
            }
        );
    }
}
