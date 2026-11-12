# Changelog

All notable changes to this project are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- `tptcli init [--local]` writes a commented starter config. It asks before overwriting; `--yes` skips the question.
- `tptcli config path|list|get|set|edit`. `list` shows each value's source. `set` checks the value before writing.
- `tptcli doctor` checks config files, resolved settings, DNS and the OS TLS trust roots. It makes no network requests.
- `tptcli completions <shell>` and `tptcli man [--dir DIR]`.
- `--config <PATH>` selects a global config file. `--version` is available.
- `tptcli schema --format mcp` lists MCP-style tools with JSON Schema inputs.
- `--select <PATH>` narrows JSON output to one field, such as `data.status`.
- `--debug` prints the internal detail of an error.
- `tptcli ping --retries N` (for refused or reset connections and timeouts) and `--header NAME: VALUE`.
  The JSON output now includes `attempts` and per-stage `timings`.
- A status line while `ping` runs on a human terminal.
- A release workflow that builds archives for five targets, writes `SHA256SUMS` and publishes a GitHub Release.
- `install.sh` and `install.ps1`, which verify the checksum before installing.

### Removed

- `SECURITY.md`. The security warning about the unaudited TLS provider stays in the README.

### Changed

- The global config file is now found at `$XDG_CONFIG_HOME/tpt/config.toml` or `%APPDATA%\tpt\config.toml`.
  The old `~/.config/tpt/config.toml` is still read when it exists.
- `output.data()` now returns `TptError` instead of `io::Error`.

- `tptcli ping` now uses a small built-in HTTP/1.1 client over `rustls` instead of `reqwest`. `tokio`,
  `reqwest`, `ring` and `sync_wrapper` are no longer dependencies. Behaviour is unchanged: redirects are followed
  (up to 10), the timeout covers the whole request, and error kinds map to the same exit codes.
- The `tptcli` binary is now named `tptcli` (it was `tpt`). The library is still `tpt`, and the config
  directory and `TPT_` environment prefix are unchanged.
- TLS for `tptcli ping` uses the pure-Rust `tpt-crypto-rustls` provider. The TLS code is **not audited**;
  see the README security warning.

## [0.1.0] - 2026-11-12

### Added

- Layered configuration (`ConfigLoader`): defaults, global file, local file, environment
  variables and CLI overrides, with merge-priority tests.
- Typed errors (`TptError`) with stable codes, exit codes and hints. The exit-code table is documented.
- `Output` for human and JSON output, with a stable JSON envelope. JSON mode writes only the envelope to stdout.
- Hand-rolled stderr logger (`TPT_LOG_LEVEL`, `--log-level`, `--verbose`, `--quiet`).
- Confirmation prompt helper (`confirm_action`) with `--yes`. It never blocks in JSON mode or without a terminal.
- `tptcli ping <url>`: HTTP GET with status, latency and timeout. Behind the `net` feature.
- `tptcli schema [COMMAND]`: JSON description of the command tree, with a snapshot test.
- `examples/basic_cmd.rs`: a minimal consumer CLI built on the library.
- Cargo features `net` (default) and `prompts` (default). The core library builds without `ring`.
- Licence files (MIT and Apache-2.0), `deny.toml` with the dependency licence policy, the release profile, and the README and contributing documents.
