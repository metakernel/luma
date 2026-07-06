use clap::Args;

use crate::{
    commands::parse::read_input,
    output::{CliError, CommandReport, EmitKind},
};

#[derive(Debug, Clone, Args)]
pub struct FmtArgs {
    #[arg(value_name = "INPUT")]
    pub input: std::path::PathBuf,

    #[arg(long, value_enum, default_value_t = EmitKind::Source)]
    pub emit: EmitKind,
}

pub fn run(args: &FmtArgs) -> Result<CommandReport, CliError> {
    let (name, text) = read_input(&args.input)?;
    let result = lyma_parser::format_str(lyma_parser::FileId(1), &name, &text);
    let payload = if result.parsed.diagnostics.is_empty() && args.emit != EmitKind::None {
        Some((
            "source",
            serde_json::Value::String(result.formatted.text.clone()),
        ))
    } else {
        None
    };

    Ok(CommandReport {
        command: "fmt",
        source: Some(result.parsed.source.source),
        diagnostics: result.parsed.diagnostics,
        payload,
        human_text: if args.emit == EmitKind::None || !result.formatted.text.is_empty() {
            Some(result.formatted.text)
        } else {
            None
        },
        success: None,
    })
}
