use allp::{app::App, cli::Cli};
use clap::Parser;
use serde_json::json;
use std::process::ExitCode;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let json_output = cli.command.json();

    match App::new().run(cli) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            if json_output {
                eprintln!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "ok": false,
                        "error": error.to_string()
                    }))
                    .unwrap_or_else(|_| "{\"ok\":false}".to_owned())
                );
            } else {
                eprintln!("✖ {error}");
            }
            ExitCode::from(error.exit_code())
        }
    }
}
