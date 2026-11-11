//! The `tpt` command-line definition. It lives in the library so that
//! `tpt schema` and tests can inspect it.

use clap::{Args, Parser, Subcommand};

use crate::config::OutputMode;
use crate::logger::LogLevel;

/// Root command, with global flags shared by every subcommand.
#[derive(Debug, Parser)]
#[command(name = "tpt", about = "TPT command-line tools", arg_required_else_help = true)]
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

    /// The subcommand to run.
    #[command(subcommand)]
    pub command: Commands,
}

/// Subcommands of `tpt`.
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Send an HTTP GET request and report the status and latency.
    #[cfg(feature = "net")]
    Ping(PingArgs),
    /// Print the command tree as JSON, for agents and tooling.
    Schema(SchemaArgs),
}

/// Arguments for `tpt ping`.
#[derive(Debug, Args)]
pub struct PingArgs {
    /// The http or https URL to request.
    pub url: String,

    /// Request timeout in seconds. Overrides the configured value.
    #[arg(long, value_name = "SECS")]
    pub timeout: Option<u64>,
}

/// Arguments for `tpt schema`.
#[derive(Debug, Args)]
pub struct SchemaArgs {
    /// Describe only this top-level command, such as `ping`.
    pub command: Option<String>,
}
