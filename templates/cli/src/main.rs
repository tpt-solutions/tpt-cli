//! {{ app_name }}: a command-line tool built on the TPT framework.
//!
//! Settings come from `~/.config/{{ app_name }}/config.toml` (or the XDG and APPDATA locations),
//! `./.{{ app_name }}.toml`, and `{{ app_name | upcase | replace: "-", "_" }}_*` environment variables.
//! Output is human text, or a JSON envelope with `--output json`. `tptcli`-style `schema` and
//! `doctor` commands come with the framework: see `tptcli --help` for the full set.

use std::ffi::OsString;
use std::process::ExitCode;

use clap::{Args, CommandFactory, Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::json;

use tpt::config::{CliOverrides, ConfigLoader, OutputMode};
use tpt::errors::TptError;
use tpt::output::Output;
use tpt::schema;

const APP: &str = "{{ app_name }}";

/// Settings for this tool. Missing keys fall back to the defaults.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    output: OutputMode,
    greeting: String,
}

#[derive(Debug, Parser)]
#[command(name = APP, about = "{{ app_name }}: a command-line tool", arg_required_else_help = true)]
struct Cli {
    /// Output format. `json` prints a strict JSON envelope to stdout.
    #[arg(long, global = true, value_enum)]
    output: Option<OutputMode>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Say hello. Replace this with your own commands.
    Greet(Greet),
    /// Print the command tree as JSON, for agents and tooling.
    Schema,
}

#[derive(Debug, Args)]
struct Greet {
    /// Who to greet.
    name: String,
}

fn main() -> ExitCode {
    let raw: Vec<OsString> = std::env::args_os().collect();
    let cli = match Cli::try_parse_from(&raw) {
        Ok(cli) => cli,
        Err(err) => err.exit(),
    };

    let overrides = CliOverrides { output: cli.output, ..Default::default() };
    let settings: Settings = match ConfigLoader::new(APP).cli_overrides(overrides.into_table()).load() {
        Ok(settings) => settings,
        Err(err) => return report(OutputMode::Human, &err),
    };

    let output = Output::new(settings.output);
    let result = match cli.command {
        Command::Greet(args) => {
            let greeting = if settings.greeting.is_empty() { "Hello" } else { settings.greeting.as_str() };
            let message = format!("{greeting}, {}!", args.name);
            output.data(&message, &json!({ "message": message }))
        }
        Command::Schema => {
            let value = schema::command_tree(&Cli::command());
            println!("{}", serde_json::to_string_pretty(&value).unwrap_or_default());
            Ok(())
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => report(settings.output, &err),
    }
}

fn report(mode: OutputMode, err: &TptError) -> ExitCode {
    Output::new(mode).error(err);
    ExitCode::from(err.exit_code())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greet_takes_a_name() {
        let cli = Cli::try_parse_from([APP, "greet", "Ada"]).unwrap();
        assert!(matches!(cli.command, Command::Greet(ref g) if g.name == "Ada"));
    }

    #[test]
    fn schema_lists_the_greet_command() {
        let tree = schema::command_tree(&Cli::command());
        let names: Vec<&str> = tree["subcommands"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"greet"));
    }
}
