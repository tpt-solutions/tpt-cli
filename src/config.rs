//! Layered configuration.
//!
//! Layers, lowest priority first:
//!
//! 1. Defaults (`Default` for the config struct)
//! 2. Global file: `~/.config/<app>/config.toml` (same path on every OS)
//! 3. Local file: `./.<app>.toml` in the current directory
//! 4. Environment variables: `<PREFIX>_<KEY>` (for example `TPT_LOG_LEVEL`)
//! 5. CLI overrides (highest priority)
//!
//! Missing files are not an error. Malformed files are, and the error names
//! the file, line and column. Tables merge key by key. Unknown keys are
//! ignored, so one file can serve several tools.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use clap::ValueEnum;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use toml::{Table, Value};

use crate::errors::{ConfigError, TptError};
use crate::logger::LogLevel;

/// The application name used by the `tpt` binary.
pub const DEFAULT_APP_NAME: &str = "tpt";

/// How results are printed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum OutputMode {
    /// Human-readable text.
    #[default]
    Human,
    /// A strict JSON envelope on stdout. Colours and prompts are disabled.
    Json,
}

impl OutputMode {
    /// The lowercase name used in config files and on the command line.
    pub fn as_str(self) -> &'static str {
        match self {
            OutputMode::Human => "human",
            OutputMode::Json => "json",
        }
    }
}

/// The configuration used by the `tpt` binary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TptConfig {
    /// Output format.
    pub output: OutputMode,
    /// Maximum log level on stderr.
    pub log_level: LogLevel,
    /// Default request timeout for network commands, in seconds.
    pub timeout_secs: u64,
}

impl Default for TptConfig {
    fn default() -> Self {
        Self { output: OutputMode::Human, log_level: LogLevel::Warn, timeout_secs: 10 }
    }
}

/// Values given on the command line. `None` means "not given", so lower layers apply.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CliOverrides {
    /// `--output`.
    pub output: Option<OutputMode>,
    /// `--log-level`, or the value implied by `--verbose` or `--quiet`.
    pub log_level: Option<LogLevel>,
    /// A command's timeout flag, such as `ping --timeout`.
    pub timeout_secs: Option<u64>,
}

impl CliOverrides {
    /// Converts the overrides into a table that [`ConfigLoader::cli_overrides`] accepts.
    pub fn into_table(self) -> Table {
        let mut table = Table::new();
        if let Some(output) = self.output {
            table.insert("output".into(), Value::String(output.as_str().into()));
        }
        if let Some(level) = self.log_level {
            table.insert("log_level".into(), Value::String(level.as_str().into()));
        }
        if let Some(secs) = self.timeout_secs {
            table.insert("timeout_secs".into(), Value::Integer(secs.try_into().unwrap_or(i64::MAX)));
        }
        table
    }
}

/// Builds a configuration struct from the layers described in the module docs.
///
/// Any application can use it. Pick an app name and the env prefix follows
/// from it (`"acme"` gives `ACME_` and `~/.config/acme/config.toml`).
#[derive(Debug, Clone)]
pub struct ConfigLoader {
    env_prefix: String,
    global_file: Option<PathBuf>,
    local_file: Option<PathBuf>,
    env: Vec<(String, String)>,
    cli: Table,
}

impl ConfigLoader {
    /// Creates a loader for `app_name`, reading the real process environment.
    pub fn new(app_name: &str) -> Self {
        Self {
            env_prefix: format!("{}_", app_name.to_ascii_uppercase().replace('-', "_")),
            global_file: std::env::home_dir()
                .map(|home| home.join(".config").join(app_name).join("config.toml")),
            local_file: Some(PathBuf::from(format!(".{app_name}.toml"))),
            env: process_env(),
            cli: Table::new(),
        }
    }

    /// Overrides the environment variable prefix, for example `"ACME_"`.
    pub fn env_prefix(mut self, prefix: &str) -> Self {
        self.env_prefix = prefix.to_owned();
        self
    }

    /// Overrides the global file path. `None` disables the global layer.
    pub fn global_file(mut self, path: Option<PathBuf>) -> Self {
        self.global_file = path;
        self
    }

    /// Overrides the local file path. `None` disables the local layer.
    pub fn local_file(mut self, path: Option<PathBuf>) -> Self {
        self.local_file = path;
        self
    }

    /// Replaces the environment used for the env layer (the process environment by default).
    pub fn env_vars<I, K, V>(mut self, vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        self.env = vars.into_iter().map(|(k, v)| (k.into(), v.into())).collect();
        self
    }

    /// Sets the CLI layer. See [`CliOverrides::into_table`].
    pub fn cli_overrides(mut self, overrides: Table) -> Self {
        self.cli = overrides;
        self
    }

    /// Resolves all layers and deserializes the result into `T`.
    pub fn load<T>(&self) -> Result<T, TptError>
    where
        T: Default + Serialize + DeserializeOwned,
    {
        let mut merged = Table::try_from(T::default())
            .map_err(|e| TptError::Internal(format!("default configuration is not serializable: {e}")))?;

        if let Some(path) = &self.global_file
            && let Some(layer) = read_layer(path)?
        {
            merge(&mut merged, layer);
        }
        if let Some(path) = &self.local_file
            && let Some(layer) = read_layer(path)?
        {
            merge(&mut merged, layer);
        }
        merge(&mut merged, self.env_layer());
        merge(&mut merged, self.cli.clone());

        Value::Table(merged).try_into::<T>().map_err(|e| {
            TptError::Config(ConfigError {
                path: None,
                line: None,
                column: None,
                message: format!("invalid configuration: {}", e.message()),
            })
        })
    }

    fn env_layer(&self) -> Table {
        let mut table = Table::new();
        for (key, value) in &self.env {
            let Some(rest) = key.strip_prefix(&self.env_prefix) else {
                continue;
            };
            if rest.is_empty() || value.is_empty() {
                continue;
            }
            table.insert(rest.to_ascii_lowercase(), parse_env_value(value));
        }
        table
    }
}

/// Reads one TOML layer. Returns `Ok(None)` if the file does not exist.
fn read_layer(path: &Path) -> Result<Option<Table>, TptError> {
    match fs::read_to_string(path) {
        Ok(text) => match toml::from_str::<Table>(&text) {
            Ok(table) => Ok(Some(table)),
            Err(err) => {
                let offset = err.span().map(|span| span.start).unwrap_or(0);
                let (line, column) = line_and_column(&text, offset);
                Err(TptError::Config(ConfigError {
                    path: Some(path.to_path_buf()),
                    line: Some(line),
                    column: Some(column),
                    message: err.message().to_owned(),
                }))
            }
        },
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(TptError::Config(ConfigError {
            path: Some(path.to_path_buf()),
            line: None,
            column: None,
            message: format!("cannot read file: {err}"),
        })),
    }
}

/// Converts a byte offset into a 1-based (line, column) pair.
fn line_and_column(text: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(text.len());
    let before = &text[..offset];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().map_or(0, |s| s.chars().count()) + 1;
    (line, column)
}

/// Env values are typed only for booleans and integers. Everything else stays a string,
/// so enum values such as `json` need no quoting.
fn parse_env_value(raw: &str) -> Value {
    match raw {
        "true" => Value::Boolean(true),
        "false" => Value::Boolean(false),
        _ => raw.parse::<i64>().map(Value::Integer).unwrap_or_else(|_| Value::String(raw.to_owned())),
    }
}

/// Deep-merges `overlay` into `base`. Overlay values win. Tables merge key by key.
pub fn merge(base: &mut Table, overlay: Table) {
    for (key, value) in overlay {
        match value {
            Value::Table(incoming) => match base.get_mut(&key) {
                Some(Value::Table(existing)) => merge(existing, incoming),
                _ => {
                    base.insert(key, Value::Table(incoming));
                }
            },
            other => {
                base.insert(key, other);
            }
        }
    }
}

fn process_env() -> Vec<(String, String)> {
    std::env::vars_os().filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

    /// A unique scratch directory under the system temp dir, removed on drop.
    struct ScratchDir(PathBuf);

    impl ScratchDir {
        fn new() -> Self {
            static COUNTER: AtomicUsize = AtomicUsize::new(0);
            let n = COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
            let dir = std::env::temp_dir().join(format!("tpt-config-test-{}-{n}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn write(&self, name: &str, contents: &str) -> PathBuf {
            let path = self.0.join(name);
            fs::write(&path, contents).unwrap();
            path
        }
    }

    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn loader(
        global: Option<PathBuf>,
        local: Option<PathBuf>,
        env: &[(&str, &str)],
        cli: CliOverrides,
    ) -> ConfigLoader {
        ConfigLoader::new("tpt")
            .env_prefix("TPT_")
            .global_file(global)
            .local_file(local)
            .env_vars(env.iter().map(|(k, v)| (k.to_string(), v.to_string())))
            .cli_overrides(cli.into_table())
    }

    #[test]
    fn defaults_apply_when_nothing_is_set() {
        let config: TptConfig = loader(None, None, &[], CliOverrides::default()).load().unwrap();
        assert_eq!(config, TptConfig::default());
    }

    #[test]
    fn missing_files_are_not_an_error() {
        let scratch = ScratchDir::new();
        let config: TptConfig = loader(
            Some(scratch.0.join("nope.toml")),
            Some(scratch.0.join(".nope.toml")),
            &[],
            CliOverrides::default(),
        )
        .load()
        .unwrap();
        assert_eq!(config, TptConfig::default());
    }

    #[test]
    fn empty_file_is_not_an_error() {
        let scratch = ScratchDir::new();
        let empty = scratch.write("empty.toml", "");
        let config: TptConfig = loader(Some(empty), None, &[], CliOverrides::default()).load().unwrap();
        assert_eq!(config, TptConfig::default());
    }

    #[test]
    fn malformed_file_reports_path_line_and_column() {
        let scratch = ScratchDir::new();
        let bad = scratch.write("bad.toml", "output = \"json\"\ntimeout_secs = = 5\n");
        let err =
            loader(Some(bad.clone()), None, &[], CliOverrides::default()).load::<TptConfig>().unwrap_err();
        match err {
            TptError::Config(ConfigError { path, line, column, .. }) => {
                assert_eq!(path, Some(bad));
                assert_eq!(line, Some(2));
                assert!(column.is_some_and(|c| c > 0));
            }
            other => panic!("expected config error, got {other:?}"),
        }
    }

    #[test]
    fn invalid_value_is_a_config_error() {
        let err = loader(None, None, &[("TPT_OUTPUT", "xml")], CliOverrides::default())
            .load::<TptConfig>()
            .unwrap_err();
        assert!(matches!(err, TptError::Config(_)));
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let scratch = ScratchDir::new();
        let file = scratch.write("extra.toml", "not_a_real_key = 1\n[other_tool]\nx = 2\n");
        let config: TptConfig = loader(Some(file), None, &[], CliOverrides::default()).load().unwrap();
        assert_eq!(config, TptConfig::default());
    }

    #[test]
    fn env_values_are_typed_for_bool_and_int_and_plain_otherwise() {
        assert_eq!(parse_env_value("42"), Value::Integer(42));
        assert_eq!(parse_env_value("true"), Value::Boolean(true));
        assert_eq!(parse_env_value("json"), Value::String("json".into()));
    }

    #[test]
    fn empty_env_values_are_ignored() {
        let config: TptConfig =
            loader(None, None, &[("TPT_LOG_LEVEL", "")], CliOverrides::default()).load().unwrap();
        assert_eq!(config.log_level, LogLevel::Warn);
    }

    /// Every combination of present layers: the highest present layer must win.
    #[test]
    fn merge_priority_matrix() {
        // Layer order, lowest first: defaults, global, local, env, cli.
        const DEFAULT: u64 = 10;
        let scratch = ScratchDir::new();

        for mask in 0u8..16 {
            let has_global = mask & 1 != 0;
            let has_local = mask & 2 != 0;
            let has_env = mask & 4 != 0;
            let has_cli = mask & 8 != 0;

            let global_value = 20;
            let local_value = 30;
            let env_value = 40;
            let cli_value = 50;

            let global = has_global.then(|| {
                scratch.write(&format!("g{mask}.toml"), &format!("timeout_secs = {global_value}\n"))
            });
            let local = has_local
                .then(|| scratch.write(&format!("l{mask}.toml"), &format!("timeout_secs = {local_value}\n")));
            let env_text = format!("{env_value}");
            let env: Vec<(&str, &str)> =
                if has_env { vec![("TPT_TIMEOUT_SECS", env_text.as_str())] } else { vec![] };
            let cli = CliOverrides { timeout_secs: has_cli.then_some(cli_value), ..Default::default() };

            let config: TptConfig = loader(global, local, &env, cli).load().unwrap();

            let expected = if has_cli {
                cli_value
            } else if has_env {
                env_value
            } else if has_local {
                local_value
            } else if has_global {
                global_value
            } else {
                DEFAULT
            };
            assert_eq!(config.timeout_secs, expected, "mask {mask:04b}");
        }
    }

    #[test]
    fn local_overrides_global_for_a_single_key_only() {
        let scratch = ScratchDir::new();
        let global = scratch.write("global.toml", "output = \"json\"\nlog_level = \"debug\"\n");
        let local = scratch.write("local.toml", "log_level = \"trace\"\n");
        let config: TptConfig =
            loader(Some(global), Some(local), &[], CliOverrides::default()).load().unwrap();
        assert_eq!(config.output, OutputMode::Json);
        assert_eq!(config.log_level, LogLevel::Trace);
    }

    #[test]
    fn line_and_column_counts_from_one() {
        assert_eq!(line_and_column("abc", 0), (1, 1));
        assert_eq!(line_and_column("ab\ncd", 4), (2, 2));
        assert_eq!(line_and_column("ab", 99), (1, 3));
    }

    #[test]
    fn custom_app_config_struct_works() {
        #[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
        #[serde(default)]
        struct Acme {
            region: String,
            retries: u32,
        }

        let config: Acme = ConfigLoader::new("acme")
            .env_prefix("ACME_")
            .global_file(None)
            .local_file(None)
            .env_vars([("ACME_REGION", "eu"), ("ACME_RETRIES", "3")])
            .load()
            .unwrap();
        assert_eq!(config, Acme { region: "eu".into(), retries: 3 });
    }
}
