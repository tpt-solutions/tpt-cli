//! The `tpt` binary. It is a thin layer over the library: parse, resolve
//! config, run, and report errors with the right exit code.

use std::ffi::OsString;
use std::process::ExitCode;
use std::time::Duration;

use clap::{CommandFactory, Parser};

use tpt::cli::{Cli, Commands};
use tpt::config::{CliOverrides, ConfigLoader, DEFAULT_APP_NAME, OutputMode, TptConfig};
use tpt::errors::{EXIT_SUCCESS, EXIT_USAGE, TptError};
use tpt::logger::{self, LogLevel};
use tpt::output::Output;
use tpt::{net, schema};

fn main() -> ExitCode {
    let raw: Vec<OsString> = std::env::args_os().collect();

    let cli = match Cli::try_parse_from(&raw) {
        Ok(cli) => cli,
        Err(err) => {
            // Help and version go to stdout and exit 0. Real usage errors exit 2.
            if !err.use_stderr() || !wants_json(&raw) {
                err.exit();
            }
            let message = err.to_string();
            let message =
                message.lines().next().unwrap_or("invalid usage").trim_start_matches("error: ").to_owned();
            Output::new(OutputMode::Json).error(&TptError::Usage(message));
            return ExitCode::from(EXIT_USAGE);
        }
    };

    let mut mode = cli.output.unwrap_or_default();
    match run(&cli, &mut mode) {
        Ok(()) => ExitCode::from(EXIT_SUCCESS),
        Err(err) => {
            let err = match err.downcast::<TptError>() {
                Ok(tpt_err) => tpt_err,
                Err(other) => TptError::Internal(format!("{other:#}")),
            };
            Output::new(mode).error(&err);
            ExitCode::from(err.exit_code())
        }
    }
}

/// Runs the command. `mode` is updated once config is resolved, so that
/// errors raised before that are reported in the right format.
fn run(cli: &Cli, mode: &mut OutputMode) -> anyhow::Result<()> {
    let timeout_secs = match &cli.command {
        Commands::Ping(args) => args.timeout,
        Commands::Schema(_) => None,
    };
    let log_level = if cli.verbose {
        Some(LogLevel::Debug)
    } else if cli.quiet {
        Some(LogLevel::Error)
    } else {
        cli.log_level
    };
    let overrides = CliOverrides { output: cli.output, log_level, timeout_secs };

    let config: TptConfig =
        ConfigLoader::new(DEFAULT_APP_NAME).cli_overrides(overrides.into_table()).load()?;
    *mode = config.output;
    logger::set_level(config.log_level);
    let output = Output::new(config.output);
    logger::set_color(output.colors_enabled());

    match &cli.command {
        Commands::Ping(args) => {
            let report = net::ping(&args.url, Duration::from_secs(config.timeout_secs))?;
            let human = format!(
                "{} {} {} in {:.2} ms (timeout {}s)",
                report.url, report.status, report.reason, report.latency_ms, report.timeout_secs
            );
            output.data(&human, &report)?;
        }
        Commands::Schema(args) => {
            // Schema output is always JSON, in every mode, and has no envelope.
            let root = Cli::command();
            let value = match &args.command {
                None => schema::command_tree(&root),
                Some(name) => schema::command_schema(&root, name).ok_or_else(|| {
                    TptError::Usage(format!(
                        "unknown command `{name}`. Run `tpt schema` to list every command."
                    ))
                })?,
            };
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
    }
    Ok(())
}

/// Returns the output format requested on the command line or in `TPT_OUTPUT`.
fn requested_output(args: &[OsString]) -> Option<String> {
    let mut iter = args.iter().map(|a| a.to_string_lossy().into_owned());
    while let Some(arg) = iter.next() {
        if arg == "--output" {
            return iter.next();
        }
        if let Some(value) = arg.strip_prefix("--output=") {
            return Some(value.to_owned());
        }
    }
    std::env::var("TPT_OUTPUT").ok()
}

/// Used when parsing fails, before config is known, to decide whether the error
/// should be a JSON envelope.
fn wants_json(args: &[OsString]) -> bool {
    requested_output(args).is_some_and(|value| value.eq_ignore_ascii_case("json"))
}
