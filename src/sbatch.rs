use std::path::{Path, PathBuf};

use crate::error::{Result, SlotdError};

#[derive(Debug, Default, Clone)]
pub struct BatchDirectives {
    pub job_name: Option<String>,
    pub partition: Option<String>,
    pub cpus_per_task: Option<u32>,
    pub mem_mb: Option<u64>,
    pub gpus: Option<u32>,
    pub output_path: Option<String>,
    pub error_path: Option<String>,
}

pub fn parse_directives(script_body: &str) -> Result<BatchDirectives> {
    let mut directives = BatchDirectives::default();

    for raw_line in script_body.lines() {
        let trimmed = raw_line.trim();
        if !trimmed.starts_with("#SBATCH") {
            continue;
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

fn apply_tokens(directives: &mut BatchDirectives, tokens: &[String]) -> Result<()> {
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        let next = tokens.get(index + 1).map(String::as_str);

        let consumed = if let Some(value) = token.strip_prefix("--job-name=") {
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
        } else if token == "--job-name" {
            directives.job_name = Some(require_value(token, next)?.to_string());
            2
        } else if let Some(value) = token.strip_prefix("--cpus-per-task=") {
            directives.cpus_per_task = Some(parse_u32("--cpus-per-task", value)?);
            1
        } else if token == "--cpus-per-task" {
            directives.cpus_per_task = Some(parse_u32(token, require_value(token, next)?)?);
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
        } else if token == "--gpus" {
            directives.gpus = Some(parse_u32(token, require_value(token, next)?)?);
            2
        } else if let Some(value) = token.strip_prefix("--output=") {
            directives.output_path = Some(value.to_string());
            1
        } else if token == "--output" {
            directives.output_path = Some(require_value(token, next)?.to_string());
            2
        } else if let Some(value) = token.strip_prefix("--error=") {
            directives.error_path = Some(value.to_string());
            1
        } else if token == "--error" {
            directives.error_path = Some(require_value(token, next)?.to_string());
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

fn split_tokens(input: &str) -> Vec<String> {
    input.split_whitespace().map(ToString::to_string).collect()
}
