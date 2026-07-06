use std::{fs, path::PathBuf};

use clap::Args;
use lyma_parser::{FileId, Parsed, parse_str};

use crate::output::{CliError, CommandReport, EmitKind};

#[derive(Debug, Clone, Args)]
pub struct ParseArgs {
    #[arg(value_name = "INPUT")]
    pub input: PathBuf,

    #[arg(long, value_enum, default_value_t = EmitKind::None)]
    pub emit: EmitKind,
}

pub fn run(args: &ParseArgs) -> Result<CommandReport, CliError> {
    let (name, text) = read_input(&args.input)?;
    let parsed = parse_str(FileId(1), &name, &text);
    Ok(build_parse_report("parse", parsed, args.emit))
}

pub fn read_input(path: &PathBuf) -> Result<(String, String), CliError> {
    let text = fs::read_to_string(path).map_err(|error| CliError {
        message: format!("failed to read '{}': {error}", path.display()),
    })?;
    Ok((path.display().to_string(), text))
}

pub fn build_parse_report(command: &'static str, parsed: Parsed, emit: EmitKind) -> CommandReport {
    let payload = match emit {
        EmitKind::None | EmitKind::Value => None,
        EmitKind::Ast => Some(("ast", crate::output::ast_to_json(&parsed.file))),
        EmitKind::Source => Some((
            "source",
            serde_json::Value::String(parsed.source.as_str().to_owned()),
        )),
    };

    CommandReport {
        command,
        source: Some(parsed.source.source),
        diagnostics: parsed.diagnostics,
        payload,
        human_text: None,
        success: None,
    }
}
