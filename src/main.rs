use allp::{app::App, cli::Cli};
use clap::Parser;
use serde_json::json;
use std::process::ExitCode;

fn main() -> ExitCode {
    let raw_args = std::env::args_os().collect::<Vec<_>>();
    if requests_version(&raw_args) {
        let verbose = raw_args.iter().skip(1).any(|argument| {
            let argument = argument.to_string_lossy();
            argument == "--verbose"
                || argument.starts_with("--verbose=")
                || (argument.starts_with('-')
                    && !argument.starts_with("--")
                    && argument.chars().any(|character| character == 'v'))
        });
        println!(
            "{}",
            if verbose {
                allp::build_identity::verbose_version_output()
            } else {
                allp::build_identity::short_version_output()
            }
        );
        return ExitCode::SUCCESS;
    }
    let cli = Cli::parse_from(raw_args);
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

fn requests_version(arguments: &[std::ffi::OsString]) -> bool {
    arguments.iter().skip(1).any(|argument| {
        let argument = argument.to_string_lossy();
        argument == "--version"
            || argument == "-V"
            || (argument.starts_with('-')
                && !argument.starts_with("--")
                && argument.chars().skip(1).any(|character| character == 'V'))
    })
}
