//! Command-line entry point for the Lyma workspace.

mod output;

mod commands {
    pub mod check;
    pub mod conformance;
    pub mod eval;
    pub mod fmt;
    #[cfg(feature = "lyba")]
    pub mod lyba;
    pub mod parse;
}

use clap::{Parser, Subcommand};
#[cfg(feature = "lyba")]
use commands::lyba::LybaArgs;
use commands::{
    check::CheckArgs, conformance::ConformanceArgs, eval::EvalArgs, fmt::FmtArgs, parse::ParseArgs,
};
use output::{CommandReport, OutputFormat, write_report};

#[derive(Debug, Parser)]
#[command(name = "lyma", version = lyma::version(), about = "LUa Markup Assembly CLI")]
struct Cli {
    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Human)]
    output: OutputFormat,

    #[command(subcommand)]
    command: Option<Command>,

    #[arg(value_name = "INPUT")]
    input: Option<std::path::PathBuf>,

    #[arg(long, value_enum, default_value_t = output::EmitKind::None)]
    emit: output::EmitKind,

    #[arg(long)]
    evaluate: bool,

    #[arg(long, value_enum)]
    engine: Option<commands::eval::EngineChoice>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Parse(ParseArgs),
    Eval(EvalArgs),
    Check(CheckArgs),
    Fmt(FmtArgs),
    Conformance(ConformanceArgs),
    #[cfg(feature = "lyba")]
    Lyba(LybaArgs),
}

fn main() {
    let cli = Cli::parse();
    let output_format = cli.output;
    let report = run(cli).unwrap_or_else(|error| CommandReport {
        command: "lyma",
        source: None,
        diagnostics: Vec::new(),
        payload: None,
        human_text: Some(error.message),
        success: Some(false),
    });
    write_report(&report, output_format);
    std::process::exit(report.exit_code());
}

fn run(cli: Cli) -> Result<CommandReport, output::CliError> {
    match cli.command {
        Some(Command::Parse(args)) => commands::parse::run(&args),
        Some(Command::Eval(args)) => commands::eval::run(&args),
        Some(Command::Check(args)) => commands::check::run(&args),
        Some(Command::Fmt(args)) => commands::fmt::run(&args),
        Some(Command::Conformance(args)) => commands::conformance::run(&args),
        #[cfg(feature = "lyba")]
        Some(Command::Lyba(args)) => commands::lyba::run(&args),
        None => commands::check::run(&CheckArgs {
            parse: ParseArgs {
                input: cli.input.ok_or_else(|| output::CliError {
                    message: "an INPUT path is required when no subcommand is supplied".to_owned(),
                })?,
                emit: cli.emit,
            },
            evaluate: cli.evaluate,
            engine: cli.engine,
        }),
    }
}
