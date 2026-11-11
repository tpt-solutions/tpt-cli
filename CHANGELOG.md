# Changelog

All notable changes to this project are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Layered configuration (`ConfigLoader`): defaults, global file, local file, environment
  variables and CLI overrides, with merge-priority tests.
- Typed errors (`TptError`) with stable codes, exit codes and hints. The exit-code table is documented.
- `Output` for human and JSON output, with a stable JSON envelope. JSON mode writes only the envelope to stdout.
- Hand-rolled stderr logger (`TPT_LOG_LEVEL`, `--log-level`, `--verbose`, `--quiet`).
- Confirmation prompt helper (`confirm_action`) with `--yes`. It never blocks in JSON mode or without a terminal.
- `tpt ping <url>`: HTTP GET with status, latency and timeout. Behind the `net` feature.
- `tpt schema [COMMAND]`: JSON description of the command tree, with a snapshot test.
- `examples/basic_cmd.rs`: a minimal consumer CLI built on the library.
- Cargo features `net` (default) and `prompts` (default). The core library builds without `ring`.
- Licence files (MIT and Apache-2.0), `deny.toml` with the dependency licence policy, the release profile, and the README, contributing and security documents.
