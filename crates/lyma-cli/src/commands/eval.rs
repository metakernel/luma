use clap::{Args, ValueEnum};

#[cfg(not(feature = "engine-omnilua"))]
use crate::output::DiagnosticFactory;
use crate::{
    commands::parse::{ParseArgs, build_parse_report, read_input},
    output::{CliError, CommandReport, EmitKind},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum EngineChoice {
    Omnilua,
}

#[derive(Debug, Clone, Args)]
pub struct EvalArgs {
    #[command(flatten)]
    pub parse: ParseArgs,

    #[arg(long, value_enum)]
    pub engine: Option<EngineChoice>,
}

pub fn run(args: &EvalArgs) -> Result<CommandReport, CliError> {
    let (name, text) = read_input(&args.parse.input)?;
    let parsed = lyma_parser::parse_str(lyma_parser::FileId(1), &name, &text);
    if !parsed.diagnostics.is_empty() {
        return Ok(build_parse_report("eval", parsed, args.parse.emit));
    }
    evaluate_parsed("eval", &name, parsed, args.parse.emit, args.engine)
}

pub fn evaluate_parsed(
    command: &'static str,
    name: &str,
    parsed: lyma_parser::Parsed,
    emit: EmitKind,
    engine: Option<EngineChoice>,
) -> Result<CommandReport, CliError> {
    #[cfg(feature = "engine-omnilua")]
    {
        let _engine = engine.unwrap_or(EngineChoice::Omnilua);
        let runtime = lyma_engine_omnilua::OmniLuaEngine::default();
        let evaluator = lyma_eval::AstEvaluator {
            engine: &runtime,
            options: lyma_eval::EvaluationOptions::default(),
        };
        let evaluated = evaluator
            .evaluate_file_with_metadata(&parsed.file, name, None)
            .map_err(|error| CliError::from_diagnostic(error.diagnostic))?;
        let payload = match emit {
            EmitKind::None => None,
            EmitKind::Ast => Some(("ast", crate::output::ast_to_json(&parsed.file))),
            EmitKind::Value => Some((
                "value",
                serde_json::Value::Array(
                    evaluated
                        .iter()
                        .map(crate::output::evaluated_document_to_json)
                        .collect(),
                ),
            )),
            EmitKind::Source => Some((
                "source",
                serde_json::Value::String(parsed.source.as_str().to_owned()),
            )),
        };

        Ok(CommandReport {
            command,
            source: Some(parsed.source.source),
            diagnostics: Vec::new(),
            payload,
            human_text: None,
            success: None,
        })
    }

    #[cfg(not(feature = "engine-omnilua"))]
    {
        let _ = (name, emit, engine);
        Ok(CommandReport::diagnostic_only(
            command,
            parsed.source.source,
            DiagnosticFactory::engine_unavailable(),
        ))
    }
}
