//! The `tpt` command-line definition. It lives in the library so that
//! `tptcli schema` and tests can inspect it.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use clap_complete::Shell;

use crate::config::OutputMode;
use crate::logger::LogLevel;

/// Root command, with global flags shared by every subcommand.
#[derive(Debug, Parser)]
#[command(name = "tptcli", version, about = "TPT command-line tools", arg_required_else_help = true)]
pub struct Cli {
    /// Output format. `json` prints a strict JSON envelope to stdout and disables prompts.
    #[arg(long, global = true, value_enum, env = "TPT_OUTPUT", value_name = "FORMAT")]
    pub output: Option<OutputMode>,

    /// Maximum log level on stderr.
    #[arg(long, global = true, value_enum, env = "TPT_LOG_LEVEL", value_name = "LEVEL")]
    pub log_level: Option<LogLevel>,

    /// Shorthand for `--log-level debug`.
    #[arg(short, long, global = true, conflicts_with = "quiet")]
    pub verbose: bool,

    /// Shorthand for `--log-level error`.
    #[arg(short, long, global = true, conflicts_with = "verbose")]
    pub quiet: bool,

    /// Answer yes to confirmation prompts.
    #[arg(short = 'y', long, global = true)]
    pub yes: bool,

    /// Use this global config file instead of the default location.
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// In JSON output, print only this field of the envelope, such as `data.status` or `data.items.0`.
    #[arg(long, global = true, value_name = "PATH")]
    pub select: Option<String>,

    /// On an error, also print the internal detail (the Rust debug form of the error).
    #[arg(long, global = true)]
    pub debug: bool,

    /// The subcommand to run.
    #[command(subcommand)]
    pub command: Commands,
}

/// Subcommands of `tptcli`.
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Send an HTTP GET request and report the status and latency.
    #[cfg(feature = "net")]
    Ping(PingArgs),
    /// Print the command tree as JSON, for agents and tooling.
    Schema(SchemaArgs),
    /// Check the setup: config files, resolved settings, DNS and TLS trust roots.
    Doctor,
    /// Write a commented starter config file.
    Init(InitArgs),
    /// Show, read, change or edit the configuration.
    #[command(subcommand)]
    Config(ConfigCommand),
    /// Print a shell completion script to stdout.
    Completions(CompletionsArgs),
    /// Print the manual page to stdout, or write one page per command to a directory.
    Man(ManArgs),
}

/// Arguments for `tptcli init`.
#[derive(Debug, Args)]
pub struct InitArgs {
    /// Write `./.tpt.toml` in the current directory instead of the global config file.
    #[arg(long)]
    pub local: bool,
}

/// Subcommands of `tptcli config`.
#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Print the path of the global config file.
    Path,
    /// List every setting with its effective value and where it comes from.
    List,
    /// Print the effective value of one setting.
    Get {
        /// The setting name, such as `timeout_secs`.
        key: String,
    },
    /// Set a value in the global config file (or the local file with `--local`).
    Set {
        /// The setting name, such as `timeout_secs`.
        key: String,
        /// The new value.
        value: String,
        /// Write `./.tpt.toml` instead of the global file.
        #[arg(long)]
        local: bool,
    },
    /// Open the global config file in `$VISUAL`, `$EDITOR` or a platform default.
    Edit,
}

/// Arguments for `tptcli completions`.
#[derive(Debug, Args)]
pub struct CompletionsArgs {
    /// The shell to generate the script for.
    #[arg(value_enum)]
    pub shell: Shell,
}

/// Arguments for `tptcli man`.
#[derive(Debug, Args)]
pub struct ManArgs {
    /// Write the pages (`tptcli.1` and one page per command) into this directory.
    #[arg(long, value_name = "DIR")]
    pub dir: Option<PathBuf>,
}

/// Arguments for `tptcli ping`.
#[derive(Debug, Args)]
pub struct PingArgs {
    /// The http or https URL to request.
    pub url: String,

    /// Request timeout in seconds. Overrides the configured value.
    #[arg(long, value_name = "SECS")]
    pub timeout: Option<u64>,

    /// Number of pings to send. Above 1, every result is printed, and the exit code is non-zero if any ping failed.
    #[arg(short = 'c', long, value_name = "N", default_value_t = 1)]
    pub count: u32,

    /// Seconds to wait between pings when --count is above 1.
    #[arg(long, value_name = "SECS", default_value_t = 1)]
    pub interval: u64,

    /// Retry a refused or reset connection, or a timeout, this many times. Waits 200 ms, 400 ms, ... between tries.
    #[arg(long, value_name = "N", default_value_t = 0)]
    pub retries: u32,

    /// Extra request header, as `Name: value`. Repeat for more headers.
    #[arg(short = 'H', long = "header", value_name = "NAME: VALUE")]
    pub headers: Vec<String>,
}

/// Arguments for `tptcli schema`.
#[derive(Debug, Args)]
pub struct SchemaArgs {
    /// Describe only this top-level command, such as `ping`.
    pub command: Option<String>,

    /// `tree` describes commands and arguments. `mcp` lists tools with JSON Schema inputs,
    /// for MCP clients and agent frameworks.
    #[arg(long, value_enum, default_value_t = SchemaFormat::Tree)]
    pub format: SchemaFormat,
}

/// Output formats for `tptcli schema`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum SchemaFormat {
    /// The command tree (default).
    Tree,
    /// MCP-style tool definitions with JSON Schema inputs.
    Mcp,
}
