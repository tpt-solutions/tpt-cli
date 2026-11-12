//! Built-in setup commands: `init`, `config`, `doctor`, `completions` and `man`.
//!
//! The logic lives in the library so it can be tested. The binary only parses
//! arguments, asks for confirmation and prints.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use clap::Command;
use clap_complete::Shell;
use serde::Serialize;
use toml::{Table, Value};

use crate::config::{ConfigLoader, DEFAULT_APP_NAME, TptConfig, read_layer};
use crate::errors::TptError;

/// The local config file name, in the current directory.
pub const LOCAL_CONFIG_FILE: &str = ".tpt.toml";

/// The starter file written by `tptcli init`. Every key is commented out, so the file
/// changes nothing until the user uncomments a line.
pub const STARTER_CONFIG: &str = "\
# tptcli configuration. Every key is optional. The values shown are the defaults.
# Precedence, lowest first: defaults, global file, local .tpt.toml,
# TPT_* environment variables, command-line flags.

# Output format: \"human\" or \"json\".
# output = \"human\"

# Maximum log level on stderr: \"error\", \"warn\", \"info\", \"debug\" or \"trace\".
# log_level = \"warn\"

# Default request timeout for network commands, in seconds.
# timeout_secs = 10
";

/// Writes the starter config to `path`, creating parent directories as needed.
pub fn write_starter_config(path: &Path) -> Result<(), TptError> {
    write_text(path, STARTER_CONFIG)
}

/// The setting names, taken from the defaults so they cannot drift from [`TptConfig`].
pub fn config_keys() -> Vec<String> {
    Table::try_from(TptConfig::default()).map(|table| table.keys().cloned().collect()).unwrap_or_default()
}

/// Turns a command-line value into TOML: booleans and integers are typed, everything else is text.
fn parse_value(raw: &str) -> Value {
    match raw {
        "true" => Value::Boolean(true),
        "false" => Value::Boolean(false),
        _ => raw.parse::<i64>().map(Value::Integer).unwrap_or_else(|_| Value::String(raw.to_owned())),
    }
}

/// Sets `key` in the config file at `path`. The file is created if it does not exist.
///
/// The new value is checked against [`TptConfig`] before anything is written, so an
/// invalid value never reaches the file. Comments in an existing file are not kept,
/// because the file is rewritten from its parsed contents.
///
/// ```
/// use tpt::admin::set_config_value;
///
/// let dir = std::env::temp_dir().join(format!("tpt-doc-{}", std::process::id()));
/// let path = dir.join("config.toml");
/// set_config_value(&path, "timeout_secs", "30").unwrap();
/// assert!(std::fs::read_to_string(&path).unwrap().contains("timeout_secs = 30"));
/// std::fs::remove_dir_all(&dir).unwrap();
/// ```
pub fn set_config_value(path: &Path, key: &str, raw: &str) -> Result<(), TptError> {
    let keys = config_keys();
    if !keys.iter().any(|k| k == key) {
        return Err(TptError::Usage(format!("unknown config key `{key}`. Valid keys: {}.", keys.join(", "))));
    }

    let value = parse_value(raw);
    let mut probe = Table::try_from(TptConfig::default()).map_err(internal)?;
    probe.insert(key.to_owned(), value.clone());
    Value::Table(probe)
        .try_into::<TptConfig>()
        .map_err(|err| TptError::Usage(format!("invalid value `{raw}` for `{key}`: {}", err.message())))?;

    let mut table = read_layer(path)?.unwrap_or_default();
    table.insert(key.to_owned(), value);
    let text = toml::to_string(&table).map_err(internal)?;
    write_text(path, &text)
}

/// Where a setting's effective value came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigEntry {
    /// The setting name.
    pub key: String,
    /// The effective value.
    pub value: serde_json::Value,
    /// `flag`, `env TPT_<KEY>`, `local file <path>`, `global file <path>` or `default`.
    pub source: String,
}

/// Resolves every setting and reports where each value came from.
///
/// `global` and `local` are the files the loader will read. `cli` holds the command-line overrides.
pub fn config_entries(
    global: Option<PathBuf>,
    local: Option<PathBuf>,
    cli: &Table,
) -> Result<Vec<ConfigEntry>, TptError> {
    let effective = ConfigLoader::new(DEFAULT_APP_NAME)
        .global_file(global.clone())
        .local_file(local.clone())
        .cli_overrides(cli.clone())
        .load::<TptConfig>()?;
    let values = serde_json::to_value(&effective).map_err(|e| TptError::Internal(e.to_string()))?;
    let local_layer = match &local {
        Some(path) => read_layer(path)?.map(|table| (path, table)),
        None => None,
    };
    let global_layer = match &global {
        Some(path) => read_layer(path)?.map(|table| (path, table)),
        None => None,
    };

    let mut entries = Vec::new();
    for key in config_keys() {
        let env_name = format!("TPT_{}", key.to_ascii_uppercase());
        let env_set = std::env::var(&env_name).is_ok_and(|v| !v.is_empty());
        let source = if cli.contains_key(&key) {
            "flag".to_owned()
        } else if env_set {
            format!("env {env_name}")
        } else if let Some((path, table)) = &local_layer
            && table.contains_key(&key)
        {
            format!("local file {}", path.display())
        } else if let Some((path, table)) = &global_layer
            && table.contains_key(&key)
        {
            format!("global file {}", path.display())
        } else {
            "default".to_owned()
        };
        entries.push(ConfigEntry { value: values[&key].clone(), key, source });
    }
    Ok(entries)
}

/// The outcome of one `doctor` check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    /// The check passed.
    Ok,
    /// Something is unusual but tptcli can still run.
    Warn,
    /// The check failed.
    Fail,
}

/// One line of the `doctor` report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Check {
    /// What was checked.
    pub name: String,
    /// The result.
    pub status: CheckStatus,
    /// Details, such as a path or an error message.
    pub detail: String,
}

/// The full `doctor` report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DoctorReport {
    /// Every check, in the order it ran.
    pub checks: Vec<Check>,
}

impl DoctorReport {
    /// `true` when no check failed. Warnings do not count as failures.
    pub fn is_healthy(&self) -> bool {
        self.checks.iter().all(|c| c.status != CheckStatus::Fail)
    }

    /// One line per check, for human output.
    pub fn render(&self) -> String {
        self.checks
            .iter()
            .map(|c| {
                let tag = match c.status {
                    CheckStatus::Ok => "ok  ",
                    CheckStatus::Warn => "warn",
                    CheckStatus::Fail => "FAIL",
                };
                format!("[{tag}] {}: {}", c.name, c.detail)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Runs the `doctor` checks. Nothing here sends data over the network.
pub fn run_doctor(global: Option<PathBuf>, local: Option<PathBuf>, cli: &Table) -> DoctorReport {
    let mut checks = vec![Check {
        name: "version".into(),
        status: CheckStatus::Ok,
        detail: format!("tptcli {}", env!("CARGO_PKG_VERSION")),
    }];
    checks.push(file_check("global config", global.as_deref()));
    checks.push(file_check("local config", local.as_deref()));

    checks.push(match config_entries(global, local, cli) {
        Ok(entries) => Check {
            name: "effective config".into(),
            status: CheckStatus::Ok,
            detail: entries
                .iter()
                .map(|e| format!("{}={} ({})", e.key, e.value, e.source))
                .collect::<Vec<_>>()
                .join(", "),
        },
        Err(err) => {
            Check { name: "effective config".into(), status: CheckStatus::Fail, detail: err.to_string() }
        }
    });

    checks.push(match std::net::ToSocketAddrs::to_socket_addrs(&("localhost", 80)) {
        Ok(addrs) => Check {
            name: "dns resolver".into(),
            status: CheckStatus::Ok,
            detail: format!("localhost resolves to {} address(es)", addrs.count()),
        },
        Err(err) => Check { name: "dns resolver".into(), status: CheckStatus::Fail, detail: err.to_string() },
    });

    #[cfg(feature = "net")]
    checks.push(tls_roots_check());

    DoctorReport { checks }
}

/// Checks that the OS trust store has usable root certificates.
#[cfg(feature = "net")]
fn tls_roots_check() -> Check {
    let result = rustls_native_certs::load_native_certs();
    let count = result.certs.len();
    let (status, detail) = match (count, result.errors.is_empty()) {
        (0, _) => (CheckStatus::Fail, "no trusted root certificates found in the OS store".to_owned()),
        (n, true) => (CheckStatus::Ok, format!("{n} trusted root certificate(s)")),
        (n, false) => (
            CheckStatus::Warn,
            format!("{n} trusted root certificate(s), {} could not be read", result.errors.len()),
        ),
    };
    Check { name: "tls trust roots".into(), status, detail }
}

fn file_check(label: &str, path: Option<&Path>) -> Check {
    let Some(path) = path else {
        return Check { name: label.into(), status: CheckStatus::Warn, detail: "no path available".into() };
    };
    if !path.exists() {
        return Check {
            name: label.into(),
            status: CheckStatus::Ok,
            detail: format!("{} not found (defaults apply)", path.display()),
        };
    }
    match read_layer(path) {
        Ok(_) => Check {
            name: label.into(),
            status: CheckStatus::Ok,
            detail: format!("{} parses", path.display()),
        },
        Err(err) => Check { name: label.into(), status: CheckStatus::Fail, detail: err.to_string() },
    }
}

/// Writes a shell completion script for `cmd` to `out`.
pub fn write_completions(shell: Shell, cmd: &mut Command, out: &mut dyn Write) {
    let name = cmd.get_name().to_owned();
    clap_complete::generate(shell, cmd, name, out);
}

/// Renders the manual page for `cmd` (roff text).
pub fn render_man(cmd: &Command) -> io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    clap_mangen::Man::new(cmd.clone()).render(&mut buf)?;
    Ok(buf)
}

/// A copy of `cmd` with a new name. clap wants a `'static` name, so the string is leaked.
/// That happens once per page in a short-lived CLI run.
fn named(cmd: &Command, name: &str) -> Command {
    let leaked: &'static str = Box::leak(name.to_owned().into_boxed_str());
    cmd.clone().name(leaked)
}

/// Writes `tptcli.1` and one page per visible subcommand (`tptcli-ping.1`, ...) into `dir`.
/// Returns the paths written.
pub fn write_man_pages(cmd: &Command, dir: &Path) -> io::Result<Vec<PathBuf>> {
    fs::create_dir_all(dir)?;
    let mut written = Vec::new();
    let mut write = |name: &str, page: &Command| -> io::Result<()> {
        let path = dir.join(format!("{name}.1"));
        let mut buf = Vec::new();
        clap_mangen::Man::new(named(page, name)).render(&mut buf)?;
        fs::write(&path, buf)?;
        written.push(path);
        Ok(())
    };
    write(cmd.get_name(), cmd)?;
    for sub in cmd.get_subcommands().filter(|s| !s.is_hide_set() && s.get_name() != "help") {
        write(&format!("{}-{}", cmd.get_name(), sub.get_name()), sub)?;
    }
    Ok(written)
}

fn write_text(path: &Path, text: &str) -> Result<(), TptError> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}

fn internal(err: impl std::fmt::Display) -> TptError {
    TptError::Internal(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use clap::CommandFactory;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A unique scratch directory, removed on drop.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            static COUNTER: AtomicUsize = AtomicUsize::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("tpt-admin-test-{}-{n}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn starter_config_is_valid_and_changes_nothing() {
        let scratch = Scratch::new();
        let path = scratch.0.join("config.toml");
        write_starter_config(&path).unwrap();
        let table = read_layer(&path).unwrap().unwrap();
        assert!(table.is_empty(), "every starter key must be commented out");
        let config: TptConfig = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(config, TptConfig::default());
    }

    #[test]
    fn set_creates_the_file_and_keeps_other_keys() {
        let scratch = Scratch::new();
        let path = scratch.0.join("nested").join("config.toml");
        set_config_value(&path, "output", "json").unwrap();
        set_config_value(&path, "timeout_secs", "25").unwrap();
        let table = read_layer(&path).unwrap().unwrap();
        assert_eq!(table["output"].as_str(), Some("json"));
        assert_eq!(table["timeout_secs"].as_integer(), Some(25));
    }

    #[test]
    fn set_rejects_unknown_keys_and_bad_values_without_writing() {
        let scratch = Scratch::new();
        let path = scratch.0.join("config.toml");
        let unknown = set_config_value(&path, "nope", "1").unwrap_err();
        assert!(matches!(unknown, TptError::Usage(ref m) if m.contains("Valid keys")));
        let bad = set_config_value(&path, "output", "xml").unwrap_err();
        assert!(matches!(bad, TptError::Usage(_)));
        let bad_type = set_config_value(&path, "timeout_secs", "soon").unwrap_err();
        assert!(matches!(bad_type, TptError::Usage(_)));
        assert!(!path.exists(), "nothing may be written for an invalid value");
    }

    #[test]
    fn config_entries_report_the_source_of_each_value() {
        let scratch = Scratch::new();
        let global = scratch.0.join("global.toml");
        fs::write(&global, "timeout_secs = 42\n").unwrap();
        let mut cli = Table::new();
        cli.insert("output".into(), Value::String("json".into()));

        let entries = config_entries(Some(global.clone()), None, &cli).unwrap();
        let find = |key: &str| entries.iter().find(|e| e.key == key).unwrap().clone();
        assert_eq!(find("output").source, "flag");
        assert_eq!(find("timeout_secs").value, serde_json::json!(42));
        assert_eq!(find("timeout_secs").source, format!("global file {}", global.display()));
        assert_eq!(find("log_level").source, "default");
    }

    #[test]
    fn doctor_flags_a_malformed_global_file() {
        let scratch = Scratch::new();
        let bad = scratch.0.join("bad.toml");
        fs::write(&bad, "timeout_secs = = 1\n").unwrap();
        let report = run_doctor(Some(bad), None, &Table::new());
        let global = report.checks.iter().find(|c| c.name == "global config").unwrap();
        assert_eq!(global.status, CheckStatus::Fail);
        assert!(!report.is_healthy());
    }

    #[test]
    fn doctor_passes_with_no_files() {
        let scratch = Scratch::new();
        let report = run_doctor(Some(scratch.0.join("none.toml")), None, &Table::new());
        assert!(report.checks.iter().all(|c| c.status != CheckStatus::Fail), "{}", report.render());
    }

    #[test]
    fn completions_are_generated_for_every_shell() {
        for shell in [Shell::Bash, Shell::Zsh, Shell::Fish, Shell::PowerShell, Shell::Elvish] {
            let mut cmd = Cli::command();
            let mut out = Vec::new();
            write_completions(shell, &mut cmd, &mut out);
            let text = String::from_utf8(out).unwrap();
            assert!(text.contains("tptcli"), "{shell:?} script must mention the binary");
        }
    }

    #[test]
    fn man_pages_are_written_for_root_and_subcommands() {
        let scratch = Scratch::new();
        let written = write_man_pages(&Cli::command(), &scratch.0).unwrap();
        let names: Vec<String> =
            written.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert!(names.contains(&"tptcli.1".to_owned()));
        assert!(names.contains(&"tptcli-config.1".to_owned()));
        assert!(names.contains(&"tptcli-doctor.1".to_owned()));
        let root = fs::read_to_string(scratch.0.join("tptcli.1")).unwrap();
        assert!(root.contains("tptcli"));
    }
}
