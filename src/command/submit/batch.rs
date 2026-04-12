use std::fs;

use crate::app::error::{Result, SlotdError};
use crate::command::args::SbatchArgs;
use crate::submit::sbatch::{BatchDirectives, parse_directives};

pub(super) fn resolve_batch_source(
    args: &SbatchArgs,
) -> Result<(String, String, BatchDirectives, Option<String>)> {
    if let Some(wrap) = &args.wrap {
        return Ok((
            "wrap".to_string(),
            format!("#!/usr/bin/env bash\n{}\n", wrap),
            BatchDirectives::default(),
            Some(wrap.clone()),
        ));
    }

    let script = args
        .script
        .as_ref()
        .ok_or_else(|| SlotdError::from("script is required"))?;
    let script_body = fs::read_to_string(script)?;
    let directives = parse_directives(&script_body)?;
    let script_name = script
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("script.sh")
        .to_string();
    Ok((script_name, script_body, directives, None))
}
