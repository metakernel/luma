use clap::Args;

use crate::{
    commands::{
        eval::{EngineChoice, evaluate_parsed},
        parse::{ParseArgs, build_parse_report, read_input},
    },
    output::{CliError, CommandReport},
};

#[derive(Debug, Clone, Args)]
pub struct CheckArgs {
    #[command(flatten)]
    pub parse: ParseArgs,

    #[arg(long)]
    pub evaluate: bool,

    #[arg(long, value_enum)]
    pub engine: Option<EngineChoice>,
}

pub fn run(args: &CheckArgs) -> Result<CommandReport, CliError> {
    let (name, text) = read_input(&args.parse.input)?;
    let parsed = luma_parser::parse_str(luma_parser::FileId(1), &name, &text);
    if args.evaluate && parsed.diagnostics.is_empty() {
        evaluate_parsed("check", &name, parsed, args.parse.emit, args.engine)
    } else {
        Ok(build_parse_report("check", parsed, args.parse.emit))
    }
}
