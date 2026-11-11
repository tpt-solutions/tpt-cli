//! A minimal consumer CLI built on the `tpt` library.
//!
//! It shows the three pieces other TPT projects use:
//! a custom config struct with its own app name, the shared output and error
//! handling, and a schema for agents.
//!
//! Try it:
//!
//! ```text
//! cargo run --example basic_cmd -- --help
//! cargo run --example basic_cmd -- greet Ada
//! ACME_GREETING="Hi" cargo run --example basic_cmd -- --output json greet Ada
//! cargo run --example basic_cmd -- schema
//! ```

use std::ffi::OsString;
use std::process::ExitCode;

use clap::{Args, CommandFactory, Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::json;

use tpt::config::{CliOverrides, ConfigLoader, OutputMode};
use tpt::errors::TptError;
use tpt::output::Output;
use tpt::schema;

/// Config for the example app. Read from `~/.config/acme/config.toml`, `./.acme.toml`, and `ACME_*` env vars.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct AcmeConfig {
    output: OutputMode,
    greeting: String,
}

#[derive(Debug, Parser)]
#[command(name = "acme", about = "Example consumer of the tpt library", arg_required_else_help = true)]
struct Acme {
    /// Output format.
    #[arg(long, global = true, value_enum, env = "ACME_OUTPUT")]
    output: Option<OutputMode>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Say hello.
    Greet(Greet),
    /// Print the command tree as JSON.
    Schema,
}

#[derive(Debug, Args)]
struct Greet {
    /// Who to greet.
    name: String,
}

fn main() -> ExitCode {
    let raw: Vec<OsString> = std::env::args_os().collect();
    let cli = match Acme::try_parse_from(&raw) {
        Ok(cli) => cli,
        Err(err) => err.exit(),
    };

    let overrides = CliOverrides { output: cli.output, ..Default::default() };
    let config: AcmeConfig = match ConfigLoader::new("acme").cli_overrides(overrides.into_table()).load() {
        Ok(config) => config,
        Err(err) => return report(OutputMode::Human, &err),
    };

    let output = Output::new(config.output);
    let result = match cli.command {
        Command::Greet(args) => {
            let greeting = if config.greeting.is_empty() { "Hello" } else { config.greeting.as_str() };
            let message = format!("{greeting}, {}!", args.name);
            output.data(&message, &json!({ "message": message })).map_err(TptError::from)
        }
        Command::Schema => {
            let value = schema::command_tree(&Acme::command());
            println!("{}", serde_json::to_string_pretty(&value).unwrap_or_default());
            Ok(())
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => report(config.output, &err),
    }
}

fn report(mode: OutputMode, err: &TptError) -> ExitCode {
    Output::new(mode).error(err);
    ExitCode::from(err.exit_code())
}
