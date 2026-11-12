//! The `tptcli` binary. It is a thin layer over the library: parse, resolve
//! config, run, and report errors with the right exit code.

use std::ffi::OsString;
use std::fmt;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::{CommandFactory, Parser};
use serde_json::{Value, json};

use tpt::admin::{self, LOCAL_CONFIG_FILE};
use tpt::cli::{Cli, Commands, ConfigCommand, ManArgs, PingArgs, SchemaFormat};
use tpt::config::{CliOverrides, ConfigLoader, DEFAULT_APP_NAME, OutputMode, TptConfig, global_config_path};
use tpt::errors::{ConfigError, EXIT_INTERNAL, EXIT_SUCCESS, EXIT_USAGE, TptError};
use tpt::logger::{self, LogLevel};
use tpt::net::PingReport;
use tpt::output::Output;
use tpt::prompts::confirm_action;
use tpt::schema;

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
            if let Some(exit) = err.downcast_ref::<ExitAfterOutput>() {
                return ExitCode::from(exit.0);
            }
            let err = match err.downcast::<TptError>() {
                Ok(tpt_err) => tpt_err,
                Err(other) => TptError::Internal(format!("{other:#}")),
            };
            Output::new(mode).error(&err);
            if cli.debug {
                eprintln!("debug: {err:?}");
            }
            ExitCode::from(err.exit_code())
        }
    }
}

/// Runs the command. `mode` is updated once config is resolved, so that
/// errors raised before that are reported in the right format.
fn run(cli: &Cli, mode: &mut OutputMode) -> anyhow::Result<()> {
    // Commands that print raw output run before config is loaded, so a broken
    // config file cannot stop shell completions or the manual page.
    match &cli.command {
        Commands::Completions(args) => {
            admin::write_completions(args.shell, &mut Cli::command(), &mut io::stdout().lock());
            return Ok(());
        }
        Commands::Man(args) => return man(args),
        _ => {}
    }

    let timeout_secs = match &cli.command {
        Commands::Ping(args) => args.timeout,
        _ => None,
    };
    let log_level = if cli.verbose {
        Some(LogLevel::Debug)
    } else if cli.quiet {
        Some(LogLevel::Error)
    } else {
        cli.log_level
    };
    let overrides = CliOverrides { output: cli.output, log_level, timeout_secs };
    let cli_table = overrides.clone().into_table();

    let global = resolve_global(cli)?;
    let local = Some(PathBuf::from(LOCAL_CONFIG_FILE));
    let config: TptConfig = ConfigLoader::new(DEFAULT_APP_NAME)
        .global_file(global.clone())
        .local_file(local.clone())
        .cli_overrides(cli_table.clone())
        .load()?;
    *mode = config.output;
    logger::set_level(config.log_level);
    let output = Output::new(config.output);
    logger::set_color(output.colors_enabled());
    tpt::output::set_select(cli.select.as_deref());

    match &cli.command {
        Commands::Ping(args) => run_ping(args, &output, &config)?,
        Commands::Schema(args) => {
            // Schema output is always JSON, in every mode, and has no envelope.
            let root = Cli::command();
            let value = match (args.format, &args.command) {
                (SchemaFormat::Mcp, top) => schema::mcp_tools(&root, top.as_deref()),
                (SchemaFormat::Tree, None) => schema::command_tree(&root),
                (SchemaFormat::Tree, Some(name)) => schema::command_schema(&root, name).ok_or_else(|| {
                    TptError::Usage(format!(
                        "unknown command `{name}`. Run `tptcli schema` to list every command."
                    ))
                })?,
            };
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        Commands::Doctor => {
            let report = admin::run_doctor(global, local, &cli_table);
            output.data(&report.render(), &report)?;
        }
        Commands::Init(args) => {
            let target = if args.local {
                PathBuf::from(LOCAL_CONFIG_FILE)
            } else {
                global.ok_or_else(no_config_dir)?
            };
            if target.exists()
                && !confirm_action(
                    &output,
                    &format!("{} already exists. Overwrite it?", target.display()),
                    cli.yes,
                )?
            {
                output.data(
                    &format!("left {} unchanged", target.display()),
                    &json!({ "path": target, "written": false }),
                )?;
                return Ok(());
            }
            admin::write_starter_config(&target)?;
            output
                .data(&format!("wrote {}", target.display()), &json!({ "path": target, "written": true }))?;
        }
        Commands::Config(action) => run_config(action, &output, global, local, &cli_table)?,
        Commands::Completions(_) | Commands::Man(_) => unreachable!("handled before config is loaded"),
    }
    Ok(())
}

fn run_config(
    action: &ConfigCommand,
    output: &Output,
    global: Option<PathBuf>,
    local: Option<PathBuf>,
    cli_table: &toml::Table,
) -> anyhow::Result<()> {
    match action {
        ConfigCommand::Path => {
            let path = global.ok_or_else(no_config_dir)?;
            let exists = path.exists();
            output.data(&path.display().to_string(), &json!({ "path": path, "exists": exists }))?;
        }
        ConfigCommand::List => {
            let entries = admin::config_entries(global, local, cli_table)?;
            let human = entries
                .iter()
                .map(|e| format!("{} = {}  ({})", e.key, display_value(&e.value), e.source))
                .collect::<Vec<_>>()
                .join("\n");
            output.data(&human, &entries)?;
        }
        ConfigCommand::Get { key } => {
            let entries = admin::config_entries(global, local, cli_table)?;
            let entry = entries.iter().find(|e| &e.key == key).ok_or_else(|| {
                TptError::Usage(format!(
                    "unknown config key `{key}`. Valid keys: {}.",
                    admin::config_keys().join(", ")
                ))
            })?;
            output.data(&display_value(&entry.value), entry)?;
        }
        ConfigCommand::Set { key, value, local: to_local } => {
            let target = if *to_local {
                PathBuf::from(LOCAL_CONFIG_FILE)
            } else {
                global.ok_or_else(no_config_dir)?
            };
            admin::set_config_value(&target, key, value)?;
            output.data(
                &format!("set {key} = {value} in {}", target.display()),
                &json!({ "path": target, "key": key, "value": value }),
            )?;
        }
        ConfigCommand::Edit => {
            let path = global.ok_or_else(no_config_dir)?;
            if !path.exists() {
                admin::write_starter_config(&path)?;
            }
            open_in_editor(&path)?;
            output.data(&format!("edited {}", path.display()), &json!({ "path": path }))?;
        }
    }
    Ok(())
}

/// Sets the exit code after output was already printed. No error envelope is added.
#[derive(Debug)]
struct ExitAfterOutput(u8);

impl fmt::Display for ExitAfterOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "exit {}", self.0)
    }
}

impl std::error::Error for ExitAfterOutput {}

/// Runs `ping`, once or `--count` times. With several pings, every result is printed, and the
/// exit code is non-zero if any ping failed.
fn run_ping(args: &PingArgs, output: &Output, config: &TptConfig) -> anyhow::Result<()> {
    let mut options = tpt::net::PingOptions::new(Duration::from_secs(config.timeout_secs));
    options.retries = args.retries;
    options.headers = args.headers.clone();
    let options = options.with_env_proxy()?;

    if args.count <= 1 {
        output.progress(&format!("requesting {}", args.url));
        let result = tpt::net::ping_with(&args.url, &options);
        output.clear_progress();
        let report = result?;
        output.data(&ping_line(&report), &report)?;
        return Ok(());
    }

    let mut results = Vec::new();
    for seq in 1..=args.count {
        if seq > 1 && args.interval > 0 {
            std::thread::sleep(Duration::from_secs(args.interval));
        }
        output.progress(&format!("ping {seq} of {}", args.count));
        let result = tpt::net::ping_with(&args.url, &options);
        output.clear_progress();
        results.push((seq, result));
    }

    let received = results.iter().filter(|(_, r)| r.is_ok()).count();
    let lines: Vec<String> = results
        .iter()
        .map(|(seq, r)| match r {
            Ok(report) => format!("#{seq} {}", ping_line(report)),
            Err(err) => format!("#{seq} error: {err}"),
        })
        .collect();
    let human = format!("{}\n{received} of {} received", lines.join("\n"), args.count);
    let items: Vec<Value> = results
        .iter()
        .map(|(seq, r)| match r {
            Ok(report) => json!({ "seq": seq, "ok": true, "report": report }),
            Err(err) => json!({
                "seq": seq,
                "ok": false,
                "error": { "code": err.code(), "message": err.to_string(), "hint": err.hint() }
            }),
        })
        .collect();
    output.data(
        &human,
        &json!({ "url": args.url, "sent": args.count, "received": received, "results": items }),
    )?;

    if received == results.len() {
        return Ok(());
    }
    let code =
        results.iter().rev().find_map(|(_, r)| r.as_ref().err()).map_or(EXIT_INTERNAL, |e| e.exit_code());
    Err(ExitAfterOutput(code).into())
}

/// The one-line human summary of a successful ping.
fn ping_line(report: &PingReport) -> String {
    format!(
        "{} {} {} in {:.2} ms (timeout {}s)",
        report.url, report.status, report.reason, report.latency_ms, report.timeout_secs
    )
}

/// Writes man pages (or prints the main page when no directory is given).
fn man(args: &ManArgs) -> anyhow::Result<()> {
    let root = Cli::command();
    match &args.dir {
        None => {
            io::Write::write_all(&mut io::stdout().lock(), &admin::render_man(&root)?)?;
        }
        Some(dir) => {
            for path in admin::write_man_pages(&root, dir)? {
                println!("{}", path.display());
            }
        }
    }
    Ok(())
}

/// The global config file: `--config` if given, else the default location.
fn resolve_global(cli: &Cli) -> anyhow::Result<Option<PathBuf>> {
    match &cli.config {
        Some(path) if path.is_file() => Ok(Some(path.clone())),
        Some(path) => Err(TptError::Config(ConfigError::in_file(path, "file not found (--config)")).into()),
        None => Ok(global_config_path(DEFAULT_APP_NAME)),
    }
}

fn no_config_dir() -> anyhow::Error {
    TptError::Usage(
        "no config location is available. Set XDG_CONFIG_HOME or HOME, or pass --config <PATH>.".into(),
    )
    .into()
}

/// Opens `path` in `$VISUAL`, `$EDITOR`, or a platform default. The variable may hold arguments.
fn open_in_editor(path: &std::path::Path) -> anyhow::Result<()> {
    let default = if cfg!(windows) { "notepad" } else { "vi" };
    let editor = ["VISUAL", "EDITOR"]
        .iter()
        .find_map(|var| std::env::var(var).ok().filter(|v| !v.trim().is_empty()))
        .unwrap_or_else(|| default.to_owned());
    let mut parts = editor.split_whitespace();
    let program = parts.next().unwrap_or(default);
    let status = std::process::Command::new(program).args(parts).arg(path).status().map_err(|e| {
        TptError::Usage(format!("cannot start editor `{program}`: {e}. Set $VISUAL or $EDITOR."))
    })?;
    if !status.success() {
        return Err(TptError::Io(io::Error::other(format!("editor `{program}` exited with {status}"))).into());
    }
    Ok(())
}

/// Plain text for a value in human output: strings without quotes.
fn display_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
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
