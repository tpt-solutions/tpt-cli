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

Prebuilt binaries are attached to each [GitHub Release](https://github.com/TPT-Solutions/tpt-cli/releases),
with a `SHA256SUMS` file. The install scripts download one, check its checksum, and install it.

```sh
# Linux and macOS
curl -fsSL https://raw.githubusercontent.com/TPT-Solutions/tpt-cli/master/install.sh | sh
```

```powershell
# Windows (PowerShell)
irm https://raw.githubusercontent.com/TPT-Solutions/tpt-cli/master/install.ps1 | iex
```

Linux, macOS and Windows are supported. Prebuilt binaries cover x86_64 and aarch64 on Linux,
Apple silicon and Intel on macOS, and x86_64 on Windows.

## Quick start

```sh
tptcli --help
tptcli init                       # write a commented starter config
tptcli doctor                     # check config files, DNS and TLS trust roots
tptcli config list                # every setting, its value and where it came from
tptcli ping https://example.com
tptcli --output json ping https://example.com
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
tptcli ping https://example.com --retries 2 --header "X-Trace: 42"
```

`--retries N` tries again after a refused or reset connection or a timeout, waiting 200 ms, 400 ms and
so on between tries. Each try gets the full timeout. `--header` (or `-H`) adds a request header and can
be repeated. `Host`, `Connection` and `Content-Length` are set by tptcli and cannot be overridden.

The JSON output includes `attempts` and a `timings` object with `dns_ms`, `connect_ms`, `tls_ms` (null for
plain http) and `ttfb_ms`.

`--count N` sends N pings, `--interval SECS` (default 1) waits between them. Every result is printed. The
exit code is non-zero if any ping failed, and it is the code of the last failure. In JSON mode the data has
`sent`, `received` and a `results` array, and nothing else is printed.

Proxies come from `HTTP_PROXY`, `HTTPS_PROXY` and `NO_PROXY` (lower-case names work too). Plain http requests
go through the proxy with the full URL. HTTPS requests ask the proxy for a `CONNECT` tunnel, so TLS still runs
end to end. Only `http://` proxy URLs are supported, and proxy credentials in the URL are not.
`NO_PROXY` takes host names, `.suffix` entries, and `*`.

On a human terminal, `ping` shows a `requesting <url>...` status line while it runs. The line is removed
when the request finishes. It is not shown with `--output json`, when output is piped, or with `--quiet`.

### `tptcli schema [COMMAND]`

Prints the command tree as JSON, so agents can find commands and arguments without parsing
`--help`. `tptcli schema ping` describes one command. The output is always JSON, in every
output mode.

`tptcli schema --format mcp` prints tool definitions instead: one per leaf command, each with a JSON Schema
`inputSchema` built from its arguments. Use it to register tptcli with an MCP client or agent framework.
Add a command name, such as `tptcli schema --format mcp ping`, to list only that command's tools.

Top-level fields: `schema_version`, `name`, `about`, `args`, `subcommands`, `exit_codes`
and `output`. Each argument has `id`, `long`, `short`, `positional`, `required`, `global`,
`type` (`string`, `bool`, `count` or `enum`), `multiple`, `values`, `default`, `env` and
`help`. Global flags are listed on the root, and single-command output includes them too.
Help and version flags are left out.

### Setup and maintenance commands

| Command | What it does |
|---|---|
| `tptcli init [--local]` | Writes a commented starter config. `--local` writes `./.tpt.toml`. Asks before overwriting; `--yes` skips the question. |
| `tptcli config path` | Prints the global config file path. |
| `tptcli config list` | Lists each setting with its effective value and its source (`default`, `global file`, `local file`, `env`, `flag`). |
| `tptcli config get <KEY>` | Prints one effective value. |
| `tptcli config set <KEY> <VALUE> [--local]` | Writes a value to the config file after checking it. Comments in an existing file are not kept. |
| `tptcli config edit` | Opens the global file in `$VISUAL`, `$EDITOR`, or `notepad` / `vi`. |
| `tptcli doctor` | Checks the tool's version, config files, resolved settings, the DNS resolver and the OS TLS trust roots. Makes no network requests. Exit code is 0 even when a check fails, so read the report. |
| `tptcli completions <SHELL>` | Prints a completion script for `bash`, `zsh`, `fish`, `powershell` or `elvish`. |
| `tptcli man [--dir DIR]` | Prints the manual page, or writes `tptcli.1` and one page per command into `DIR`. |

Shell completion examples:

```sh
tptcli completions bash > ~/.local/share/bash-completion/completions/tptcli
tptcli completions zsh > "${fpath[1]}/_tptcli"
```

```powershell
tptcli completions powershell | Out-String | Invoke-Expression   # current session only
```

## Configuration

Settings resolve in this order. Later items win:

1. Built-in defaults
2. Global file (see below)
3. Local file: `./.tpt.toml` in the current directory
4. Environment variables: `TPT_<KEY>`, for example `TPT_LOG_LEVEL=debug`
5. Command-line flags, including `--config <PATH>` to use a different global file

The global file is the first of these that exists:

1. `$XDG_CONFIG_HOME/tpt/config.toml`, when `XDG_CONFIG_HOME` is set
2. `%APPDATA%\tpt\config.toml` on Windows
3. `~/.config/tpt/config.toml` (the path used by earlier versions, still read)

If none exists, `tptcli init` and `tptcli config set` create the first one. Run `tptcli config path`
to see which file is in use.

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

### Narrowing the output with `--select`

`--select <PATH>` prints one part of the JSON envelope. The path is dot-separated and starts at the
top of the envelope. Array items are addressed by index.

```sh
tptcli --output json --select data.status ping https://example.com   # prints: 200
tptcli --output json --select data.timings.ttfb_ms ping https://example.com
```

A path that does not exist is a usage error. `--select` has no effect in human output.

### Debugging an error with `--debug`

On an error, `--debug` also prints the Rust debug form of the error to stderr, which includes the
internal detail that the normal message leaves out. Use it when reporting a bug.

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
> traffic. Report suspected flaws in the TLS path as security issues, not on the public tracker.

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

See [CONTRIBUTING.md](CONTRIBUTING.md).
