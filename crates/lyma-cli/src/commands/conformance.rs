use clap::Args;

use crate::output::{CliError, CommandReport};

#[derive(Debug, Clone, Args)]
pub struct ConformanceArgs {
    #[arg(long)]
    pub all_features: bool,
}

pub fn run(args: &ConformanceArgs) -> Result<CommandReport, CliError> {
    let mut command = std::process::Command::new("cargo");
    command.arg("test").arg("--test").arg("conformance");
    if args.all_features {
        command.arg("--all-features");
    }

    let output = command.output().map_err(|error| CliError {
        message: format!("failed to run cargo test --test conformance: {error}"),
    })?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let human_text = format!("{stdout}{stderr}");

    Ok(CommandReport {
        command: "conformance",
        source: None,
        diagnostics: Vec::new(),
        payload: Some((
            "result",
            serde_json::json!({
                "success": output.status.success(),
                "code": output.status.code(),
                "stdout": stdout,
                "stderr": stderr,
            }),
        )),
        human_text: Some(human_text),
        success: Some(output.status.success()),
    })
}
