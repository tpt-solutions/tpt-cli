# tpt-cli

[![CI](https://github.com/tpt-solutions/tpt-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/tpt-solutions/tpt-cli/actions/workflows/ci.yml)

`tpt-cli` is a small Rust framework and the `tpt` command for building command-line
tools that people and AI agents can both use. It is built and maintained by TPT Solutions.

- **Fast to compile and small.** Heavy features (networking, prompts) sit behind Cargo features.
- **Agent-ready.** `--output json` gives a strict JSON envelope on stdout. `tptcli schema`
  describes every command as JSON.
- **Permissively licensed.** `MIT OR Apache-2.0`. See [Licensing](#licensing).

## Install

From source (Rust 1.88 or newer):

```sh
cargo install --path .
```

Or build it in place:

```sh
cargo build --release   # binary at target/release/tptcli
```

Linux, macOS and Windows are supported.

## Quick start

```sh
tpt --help
tptcli ping https://example.com
tpt --output json ping https://example.com
tptcli schema
```

Example output of `tptcli ping`:

```text
https://example.com/ 200 OK in 84.21 ms (timeout 10s)
```

### `tptcli ping <url>`

Sends an HTTP GET request and reports the status code and latency. Any HTTP status counts
as a reply, so a `503` is reported with exit code 0. Only `http` and `https` URLs are
accepted. Redirects are followed.

```sh
tptcli ping https://example.com --timeout 3
```

### `tptcli schema [COMMAND]`

Prints the command tree as JSON, so agents can find commands and arguments without parsing
`--help`. `tptcli schema ping` describes one command. The output is always JSON, in every
output mode.

Top-level fields: `schema_version`, `name`, `about`, `args`, `subcommands`, `exit_codes`
and `output`. Each argument has `id`, `long`, `short`, `positional`, `required`, `global`,
`type` (`string`, `bool`, `count` or `enum`), `multiple`, `values`, `default`, `env` and
`help`. Global flags are listed on the root, and single-command output includes them too.
Help and version flags are left out.

## Configuration

Settings resolve in this order. Later items win:

1. Built-in defaults
2. Global file: `~/.config/tpt/config.toml`
3. Local file: `./.tpt.toml` in the current directory
4. Environment variables: `TPT_<KEY>`, for example `TPT_LOG_LEVEL=debug`
5. Command-line flags

The global path is the same on every OS. Use `~/.config/tpt/config.toml`
on Windows too, which resolves to `%USERPROFILE%\.config\tpt\config.toml`.

Missing files are not an error. A malformed file is an error, and it reports the file,
line and column.

Example `config.toml`:

```toml
output = "human"        # "human" or "json"
log_level = "warn"      # error | warn | info | debug | trace
timeout_secs = 10       # default timeout for network commands
```

Keys that a tool does not know are ignored. That lets one file serve several tools.

Environment variables:

| Variable | Key | Example |
|---|---|---|
| `TPT_OUTPUT` | `output` | `json` |
| `TPT_LOG_LEVEL` | `log_level` | `debug` |
| `TPT_TIMEOUT_SECS` | `timeout_secs` | `5` |

## Agent and machine mode (`--output json`)

Pass `--output json` (or set `TPT_OUTPUT=json`, or `output = "json"` in a config file).
In this mode:

- Stdout holds one strict JSON document, and nothing else.
- Colours, spinners and prompts are turned off. A prompt that is needed fails with a clear
  error. It never blocks.
- All logs go to stderr.

Success:

```json
{"ok": true, "data": {"url": "https://example.com/", "status": 200, "reason": "OK", "latency_ms": 84.21, "timeout_secs": 10}}
```

Failure:

```json
{"ok": false, "error": {"code": "dns", "message": "could not reach https://nope.invalid/: ...", "hint": "Check the host name for typos, and your DNS or proxy settings."}}
```

Error `code` values: `usage`, `config`, `dns`, `tls`, `connection`, `timeout`, `io`,
`aborted` and `internal`. Check the exit code and the `code` field, not the message text.

## Exit codes

| Code | Name | Meaning |
|---:|---|---|
| 0 | success | The command succeeded. |
| 1 | internal | An unexpected internal error (please report it). |
| 2 | usage | Invalid command-line usage. |
| 3 | config | A configuration file or value is invalid. |
| 4 | dns | The host name could not be resolved. |
| 5 | tls | The TLS handshake or certificate check failed. |
| 6 | connection | The connection was refused, reset or unreachable. |
| 7 | timeout | The request timed out. |
| 8 | io | A local file or I/O operation failed. |
| 130 | aborted | The user aborted the operation. |

The same table is in `tptcli schema` under `exit_codes`.

## Library usage

Other TPT projects can use `tpt-cli` as a library. Turn off default features to leave out
networking and prompts:

```toml
[dependencies]
tpt-cli = { version = "0.1", default-features = false }
```

`examples/basic_cmd.rs` is a complete small CLI. It shows a custom config struct with its
own app name and env prefix, the shared output and error handling, and a schema command.
Run it with `cargo run --example basic_cmd -- --help`.

Main modules:

- `config`: `ConfigLoader` (any app name and env prefix) and the `TptConfig` struct
- `errors`: `TptError`, with `code()`, `exit_code()` and `hint()`
- `output`: `Output` for results, messages and errors
- `prompts`: `confirm_action`, which honours `--yes` (feature `prompts`)
- `logger`: the stderr logger
- `net`: `ping` (feature `net`)
- `schema`: `command_tree` and `command_schema` for any clap `Command`

## Cargo features

| Feature | Default | Adds | Notes |
|---|---|---|---|
| `net` | yes | `rustls`, `tpt-crypto-rustls`, `url`, `http` (no async runtime) | TLS crypto comes from the pure-Rust `tpt-crypto-rustls` provider. |
| `prompts` | yes | `dialoguer` | Interactive confirmation prompts. |

`cargo build --no-default-features` builds the core library with no TLS and no
network code. The `tptcli` binary requires `net`. Neither build contains `ring` or any C crypto.

> **Security warning: the TLS provider is not audited.** `tptcli ping` uses
> `tpt-crypto-rustls`, a rustls provider built on `tpt-crypto`. Both are hand-written pure-Rust
> crypto that has not been independently audited or formally verified. Treat HTTPS results from
> `tptcli ping` as best-effort until the audit is done. Do not rely on them to protect sensitive
> traffic. See [SECURITY.md](SECURITY.md).

## Licensing

`tpt-cli` is dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
Copyright (c) 2026 TPT Solutions.

### Dependency licence policy

- Allowed: MIT, ISC, BSD-2-Clause, BSD-3-Clause, Zlib, Unicode-3.0, CC0-1.0, Unlicense.
- Apache-2.0 is accepted only as one option of an `OR` expression, such as `MIT OR Apache-2.0`.
  It is not accepted on its own or inside an `AND` expression.
- Copyleft (GPL, LGPL, AGPL), MPL and CDLA are not accepted.
- Every exception is listed in [`deny.toml`](deny.toml) with a reason. Check with
  `cargo deny check licenses`.

### Dependency exceptions

No licence exceptions remain. The one documented advisory exception is in [`deny.toml`](deny.toml):

- `RUSTSEC-2023-0071` (`rsa`, Marvin timing attack): `rsa` is used by `tpt-crypto-rustls` for
  server-certificate verification only, with no private-key operations. Review by 2027-02-10.

`ring` is no longer a dependency. It was replaced by `tpt-crypto-rustls` (`todo.md`, Phase 9).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Report security issues as described in
[SECURITY.md](SECURITY.md).
