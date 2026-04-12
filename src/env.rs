use std::fs;
use std::path::Path;

use crate::error::{Result, SlotdError};

pub(crate) fn resolve_export_env(
    export: Option<&str>,
    export_file: Option<&Path>,
) -> Result<Vec<(String, String)>> {
    let mut env = Vec::<(String, String)>::new();
    if let Some(path) = export_file {
        let contents = fs::read_to_string(path)?;
        env.extend(parse_export_file_contents(&contents)?);
    }
    if let Some(spec) = export {
        env = resolve_export_spec(spec, &env)?;
    }
    env.sort_by(|a, b| a.0.cmp(&b.0));
    env.dedup_by(|a, b| a.0 == b.0);
    Ok(env)
}

pub(crate) fn parse_env_flag(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes"
    )
}

pub(crate) fn resolve_export_spec(
    spec: &str,
    seed: &[(String, String)],
) -> Result<Vec<(String, String)>> {
    let trimmed = spec.trim();
    if trimmed.eq_ignore_ascii_case("none") {
        return Ok(Vec::new());
    }

    let mut env = if trimmed.eq_ignore_ascii_case("all") || trimmed.starts_with("ALL,") {
        std::env::vars().collect::<Vec<_>>()
    } else {
        seed.to_vec()
    };
    let entries = trimmed
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .collect::<Vec<_>>();

    for entry in entries {
        if entry.eq_ignore_ascii_case("all") {
            continue;
        }
        if entry.eq_ignore_ascii_case("none") {
            env.clear();
            continue;
        }
        if let Some((key, value)) = entry.split_once('=') {
            validate_env_name(key)?;
            upsert_env_pair(&mut env, key, value);
            continue;
        }
        validate_env_name(entry)?;
        let value = std::env::var(entry).unwrap_or_default();
        upsert_env_pair(&mut env, entry, &value);
    }

    Ok(env)
}

fn parse_export_file_contents(contents: &str) -> Result<Vec<(String, String)>> {
    let mut env = Vec::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(SlotdError::from(format!(
                "invalid export-file entry: {line}"
            )));
        };
        validate_env_name(key)?;
        env.push((key.to_string(), value.to_string()));
    }
    Ok(env)
}

fn upsert_env_pair(env: &mut Vec<(String, String)>, key: &str, value: &str) {
    if let Some(existing) = env.iter_mut().find(|(name, _)| name == key) {
        existing.1 = value.to_string();
    } else {
        env.push((key.to_string(), value.to_string()));
    }
}

fn validate_env_name(value: &str) -> Result<()> {
    if value.is_empty()
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        || value.chars().next().is_some_and(|ch| ch.is_ascii_digit())
    {
        return Err(SlotdError::from(format!(
            "invalid environment variable name: {value}"
        )));
    }
    Ok(())
}
