# tpt-cli TODO

Publisher: TPT Solutions. Licence: MIT OR Apache-2.0 (dual). Binary: `tptcli` (renamed from `tpt` so the executable is not confused with the library and the config dir). Crate: `tpt-cli` (lib + bin).

Decisions locked in:
- Implementation notes (from building Phases 0-7):
  - MSRV is 1.88, not 1.85, because the code uses let-chains. Tested on 1.88 and stable locally.
  - `dirs` was dropped for `std::env::home_dir`. `dirs` 6 and 7 both pull in `option-ext` (MPL-2.0), which the policy forbids.
  - The global config path is `~/.config/tpt/config.toml` on every OS, so Windows and macOS match Linux.
  - `reqwest` is 0.12, not 0.13. The 0.13 `rustls` feature uses `aws-lc-rs`, which is C-based. 0.12 uses `ring` with `rustls-native-certs`.
  - Two `deny.toml` exceptions were added: `ryu` (take its Apache-2.0 OR option) and `sync_wrapper` (Apache-2.0 only, needed by reqwest). The `sync_wrapper` one needs the maintainer's sign-off.
  - `ping --timeout` sets the timeout. The report includes `timeout_secs`.
- Dependency licence policy: allow MIT, ISC, BSD-2/3, Zlib, Unicode-3.0, CC0, Unlicense. Apache-2.0 is accepted only as an "OR" alternative in a dual licence, never Apache-only or `AND`. No copyleft/MPL/CDLA.
- Networking sits behind a `net` Cargo feature. The core lib stays fully MIT-clean. The `net` feature carries one documented exception: `ring` (Apache-2.0 AND ISC).
- Use `rustls-native-certs` (OS cert store) instead of `webpki-roots`
- Long term: replace `ring` (see the last phase)
- Single crate, lib `tpt` + `tptcli` binary
- `tpt ping <url>` does an HTTP GET and reports status and latency
- Linux, macOS and Windows supported
- Hand-rolled stderr logger (no logging crate)
- GitHub + GitHub Actions
- `tpt schema` emits a custom JSON command tree

---

## Phase 0: Project setup and licensing
- [x] Create `Cargo.toml` (name `tpt-cli`, edition 2024, `rust-version`, `license = "MIT OR Apache-2.0"`, authors/repository metadata for TPT Solutions)
- [x] Add `LICENSE-MIT` (copyright TPT Solutions)
- [x] Add `LICENSE-APACHE` (copyright TPT Solutions)
- [x] Add `.gitignore`, `rustfmt.toml`, `rust-toolchain.toml` (stable)
- [x] `git init` and make the first commit
- [x] Write the licence section in `README.md` (standard "MIT or Apache-2.0, at your option" + contribution clause)
- [x] Add `deny.toml` for cargo-deny: allowlist MIT, ISC, BSD-2/3, Zlib, Unicode-3.0, CC0, Unlicense; Apache-2.0 only inside `OR` expressions; deny copyleft, MPL and CDLA
- [x] Run `cargo deny check licenses` against the full dependency tree, with and without the `net` feature
- [x] Verify each crate's licence from its actual `Cargo.toml` (do not rely on memory), especially ring, rustls, rustls-native-certs, rustls-webpki
- [x] Audit every direct and transitive dependency for Apache-only or `AND Apache` licences
- [x] Add a single documented `ring` exception to `deny.toml`, scoped to the `net` feature only
- [x] Replace or exempt any other dependency that fails the audit (document each exception in `deny.toml`)
- [x] Set the release profile: `opt-level = "z"` or `3` (benchmark), `lto`, `codegen-units = 1`, `panic = "abort"`, `strip`
- [x] Record baseline metrics: clean build time, incremental build time, binary size

## Phase 1: Skeleton (Spec Milestone 1)
- [x] Add dependencies with minimal features:
  - [x] `clap` (`default-features = false`; `derive`, `env`, `help`, `std`)
  - [x] `serde`, `toml`, `dirs`
  - [x] `tokio` (`rt` only, no multi-thread)
  - [x] `reqwest` (`default-features = false`, rustls only)
  - [x] `thiserror`, `anyhow`, `serde_json`
  - [x] `dialoguer`, `console`
- [x] Gate heavier features (reqwest, tokio, dialoguer) behind Cargo features
- [x] `net` feature: reqwest + tokio + rustls + `rustls-native-certs` (no `webpki-roots`); the only feature that pulls in ring
- [x] Verify `cargo tree --no-default-features` contains no ring and no Apache-only crates
- [x] The `tpt` binary enables `net`; library consumers can opt out with `default-features = false`
- [x] Create the `src/` layout: `main.rs`, `lib.rs`, `config.rs`, `errors.rs`, `output.rs`, `prompts.rs`, plus `examples/basic_cmd.rs`
- [x] `errors.rs`: typed `TptError` enum (thiserror) covering config, network, IO, usage and user-abort errors
- [x] `errors.rs`: map each error variant to a specific exit code
- [x] `errors.rs`: document the exit-code table
- [x] `errors.rs`: actionable message formatting with a hint line
- [x] `errors.rs`: `anyhow` used only at the binary boundary
- [x] `config.rs`: define the `Config` struct (serde) with defaults
- [x] `config.rs`: locate the global config at `~/.config/tpt/config.toml` using `dirs` (check the Windows and macOS equivalents behave sensibly)
- [x] `config.rs`: load `.tpt.toml` from the current directory
- [x] `config.rs`: read env vars with the `TPT_` prefix (e.g. `TPT_LOG_LEVEL`)
- [x] `config.rs`: merge in priority order: CLI args > env > local > global > defaults
- [x] `config.rs`: clean error for malformed TOML (file path, line and column)
- [x] `config.rs`: missing files are not an error
- [x] Unit tests for each merge-priority combination
- [x] Unit tests for missing, empty and malformed config files
- [x] Milestone check: CLI, env and TOML merge proven by tests

## Phase 2: UX and output (Spec Milestone 2)
- [x] `output.rs`: global `--output <human|json>` flag (also settable via env and config)
- [x] `output.rs`: `Output` abstraction (`success`, `data`, `info`, `warn`, `error`)
- [x] `output.rs`: JSON mode writes strict JSON to stdout only; all human logs go to stderr
- [x] `output.rs`: JSON mode disables colours, spinners and prompts
- [x] `output.rs`: define a stable JSON envelope (e.g. `{ "ok": true, "data": ... }` / `{ "ok": false, "error": { "code", "message", "hint" } }`)
- [x] `output.rs`: respect `NO_COLOR` and TTY detection for colours
- [x] Hand-rolled stderr logger: levels error/warn/info/debug/trace
- [x] Logger honours `TPT_LOG_LEVEL`, config and a `--verbose`/`--quiet` flag
- [x] `prompts.rs`: standardized "Confirm Action" prompt (dialoguer wrapper)
- [x] `prompts.rs`: `--yes` flag to skip prompts
- [x] `prompts.rs`: non-TTY and JSON mode never block; they fail with a clear error or use the default
- [x] `prompts.rs`: consistent theme via `console`
- [x] `main.rs`: clap root command with global flags
- [x] `main.rs`: single-threaded tokio runtime bootstrap
- [x] `main.rs`: top-level error handler that prints through `output.rs` and sets the exit code
- [x] `main.rs`: `tpt ping <url>` command (HTTP GET, status code, latency, timeout flag)
- [x] `ping`: human output and JSON output
- [x] `ping`: network failures, DNS and TLS errors map to typed errors with hints
- [x] Integration tests for `ping` (local test server) in human and JSON modes
- [x] Integration tests that assert exit codes and that stdout is valid JSON in JSON mode
- [x] Milestone check: `tpt ping` works end to end

## Phase 3: Agentic polish (Spec Milestone 3)
- [x] Define the JSON schema format for the command tree (name, about, args, flags, types, defaults, env vars, required, subcommands)
- [x] Implement `tpt schema` by walking clap's `Command` structure
- [x] `schema` always outputs JSON (no human formatting needed)
- [x] Include global flags, exit-code table and output envelope in the schema output
- [x] Support `tpt schema <command>` to describe a single command
- [x] Snapshot test for the schema output
- [x] Test that every registered command appears in the schema (no drift)
- [x] Document the schema format in the README
- [x] Milestone check: an agent can discover all commands from `tpt schema` alone

## Phase 4: Library API for other TPT repos
- [x] Design the public API in `lib.rs` (re-exports, prelude, feature flags)
- [x] Make config loading reusable by other CLIs (custom config struct, custom app name and prefix)
- [x] Make the output, error and prompt helpers usable without the `tpt` binary's commands
- [x] Make `schema` generation reusable for any clap `Command`
- [x] `examples/basic_cmd.rs`: minimal consumer CLI built on the lib
- [x] Verify the example builds and runs in CI (CI test job now runs `cargo run --example basic_cmd -- greet Ada`; not yet seen green on GitHub)
- [ ] Rustdoc on every public item, with doctests (partial: `missing_docs` is enforced and clean; doctests now cover the lib crate, `config::merge`, `CliOverrides::into_table`, `OutputMode::as_str`, `LogLevel::as_str`, the logger's `set_level`, `level`, `enabled`, `set_color` and `log`, all `ConfigLoader` builder methods and `load`, `Output::new`, `mode`, `is_json`, `colors_enabled` and `success`, `output::success_envelope` and `error_envelope`, `errors::TptError::exit_code` and `hint`, `error_chain`, `schema::command_tree` and `command_schema`, and `prompts::confirm_action`. Not covered: `net::ping` (needs a network), `Output::data`, `error`, `info` and `warn` (they write to stdout or stderr, so a doctest would print), and the `logger::error`, `warn`, `info`, `debug` and `trace` wrappers)
- [x] Review the public surface for semver stability (`#[non_exhaustive]` added to `TptError`, `NetworkKind`, `ConfigError`, `LogLevel`, `OutputMode` and `PingReport`, since they are likely to grow. Not yet marked: `TptConfig` and `CliOverrides` have public fields that consumers build with struct literals, so changing them later would break callers. Decide before 0.1.0)
- [x] Decide feature-flag defaults so consumers pay only for what they use

## Phase 5: Quality, performance and compliance
- [x] `cargo fmt --check` clean
- [x] `cargo clippy -- -D warnings` clean
- [x] Full test suite passes on Linux, macOS and Windows (CI test matrix is green; stable and 1.88.0)
- [x] Confirm the dependency tree has no OpenSSL or other C dependencies (`cargo tree`) (no openssl or native-tls; `ring` only with the `net` feature)
- [x] Re-run `cargo deny check` (licenses, bans, advisories) on the final tree
- [x] Generate a third-party licence list for the repo (`THIRD-PARTY-LICENSES.md`, from `cargo deny list`)
- [ ] Compare final binary size and build time against the Phase 0 baseline (blocked: the baseline numbers were ticked off as recorded but never written down, so there is nothing to compare against. Record the Phase 0 values from a clean checkout, if they still exist, or re-baseline and note that in the README)
- [x] Remove unused dependencies and features (checked by hand: every direct dependency is referenced in `src`, `tests` or `examples`; `cargo machete` and `cargo udeps` not installed)
- [x] Check there are no panics on bad input (manual runs: bad flags, zero timeout, invalid URL, malformed TOML, wrong-typed config, refused connection, DNS failure; all exit with the mapped code and valid JSON in JSON mode)
- [ ] Verify Windows terminal behaviour (colours, prompts, paths, config location)
- [x] Set the MSRV and test against it in CI

## Phase 6: CI/CD (GitHub Actions)
- [x] Create the GitHub repo under TPT Solutions (`tpt-solutions/tpt-cli` exists and PR #1 has merged)
- [x] Workflow: fmt + clippy
- [x] Workflow: test matrix (Linux, macOS, Windows; stable + MSRV)
- [x] Workflow: `cargo deny check`
- [x] Workflow: build with the release profile and report binary size
- [x] Add `dependabot.yml` or equivalent for dependency updates
- [x] Add a CI badge to the README
- [ ] Protect the main branch with required checks

## Phase 7: Documentation and community files
- [x] README: overview, install, quick start, `ping` and `schema` examples
- [x] README: configuration resolution order and example `config.toml` / `.tpt.toml`
- [x] README: agent/machine mode (`--output json`) and the JSON envelope
- [x] README: exit-code table
- [x] README: library usage guide for other TPT repos
- [x] README: licensing and the dependency-licence policy
- [x] README: state clearly that the `net` feature pulls in ring (Apache-2.0 AND ISC) and the core lib does not
- [x] `CHANGELOG.md`
- [x] `CONTRIBUTING.md` (including the dual-licence contribution clause)
- [x] `SECURITY.md`
- [x] Issue templates (bug report, feature request, security link in `.github/ISSUE_TEMPLATE`). No PR template: pull requests are not accepted. `CODE_OF_CONDUCT.md` dropped by decision.

## Phase 8: Release (v0.1.0)
- [x] Final review against `spec.txt` (every requirement ticked off). Deviations, all deliberate or still open: the binary is `tpt`, not `tpt-cli` (decided in the locked list above); `dirs` was dropped for `std::env::home_dir` (decided above); spinners are not implemented, though the spec and README mention them (nothing to turn off yet; open gap)
- [x] Bump the version to 0.1.0 and update the changelog
- [ ] Tag `v0.1.0`
- [ ] Draft the GitHub release notes
- [ ] Smoke test a fresh install and build on Linux, macOS and Windows
- [ ] Post-release: open the backlog for the next version

## Phase 9: Long term, ring replacement (separate project/crate)
- [ ] List the exact workarounds hit so far and which ring features tpt actually needs (likely only a rustls CryptoProvider: TLS 1.3 AEADs, X25519, ECDSA/Ed25519/RSA verify, HKDF, HMAC, SHA-2, RNG)
- [ ] Decide the approach: write primitives from scratch (decided; supersedes "wrap RustCrypto"). They already exist in sibling repos `tpt-crypto` (primitives) and `tpt-crypto-rustls` (rustls 0.23 `CryptoProvider`, client-only, not audited). Reuse them, do not start a new crate
- [x] Gap: RSA verify in `tpt-crypto-rustls` still uses the `rsa` and `sha2` crates. (Done on branch `rsa-verify` in both repos. `tpt-crypto-sig::rsa` existed already with KATs; the PKCS#1 DER parser was added and `tpt-crypto-rustls` now uses it. Not pushed.) Add from-scratch RSA PKCS#1 v1.5 + PSS verify to `tpt-crypto-sig` (bignum modexp in `tpt-crypto-field`), then drop both crates (also clears RUSTSEC-2023-0071)
- [ ] Gap: `tpt-crypto` ECDSA verify rejects high-S signatures; fix upstream and remove the low-S workaround in `tpt-crypto-rustls`
- [ ] Gap (full parity): add signing (ECDSA, Ed25519, RSA-PSS) to `tpt-crypto-sig` plus a `SigningKey`/`KeyProvider` in `tpt-crypto-rustls`. Do ECDSA/Ed25519 first; RSA signing is the riskiest. tpt-cli does not need it
- [ ] Replace the dev-only `rustls-rustcrypto` test server with a rustls server on the new signing support
- [ ] Retire the duplicate `tpt-crypto/examples/src/rustls_provider.rs` in favour of `tpt-crypto-rustls`
- [x] Swap tpt-cli first, using the existing client-only provider: reqwest feature `rustls-tls-native-roots-no-provider`, `rustls::ClientConfig::builder_with_provider(tpt_crypto_rustls::provider())`, `reqwest::ClientBuilder::use_preconfigured_tls` in `src/net.rs` (done at `tpt-crypto-rustls` `e6b5fcb`. Live HTTPS checks pass against example.com and google.com. Expired and self-signed certificates are rejected with exit 5, the TLS code)
- [x] Allow the `tpt-crypto-rustls` git source in `deny.toml` `[sources]` (`unknown-git = "deny"`), add `ring` to `[bans].deny`, reword the "Use ring" ban reasons (done: `allow-git` lists both TPT repos; `ring` is banned; `cargo deny check` passes. `RUSTSEC-2023-0071` is ignored with the same reason as tpt-mcpbox, until RSA verify moves to tpt-crypto)
- [x] Decision (user): replace `ring` immediately, with no opt-in period. Record the unaudited-crypto risk in the README and SECURITY until the audit is done (done: README security warning and SECURITY.md design note)
- [ ] Replace `sync_wrapper` (Apache-2.0 only, pulled in by reqwest 0.12) too. Swapping the TLS provider does not remove it. Check first which crates depend on it (`cargo tree -i sync_wrapper`): reqwest, or also hyper-util/tower (checked: `sync_wrapper` is pulled in by `reqwest` and `tower`, so it stays until reqwest is replaced; see the item below)
- [x] Decide the replacement for reqwest: (a) a minimal HTTP/1.1 GET client over `std::net::TcpStream` plus `rustls::StreamOwned` with the tpt-crypto provider, which needs no tokio or async and is enough for `tpt ping` (redirects, timeouts, http and https); or (b) hyper without reqwest, if it can be built with no Apache-only crates. Recommendation: (a), since it also drops `tokio` and shrinks the binary. DECIDED and done: option (a), implemented in `src/net.rs`
- [x] Implement the replacement in `src/net.rs`, keeping `PingReport`, redirect following, `--timeout` and the typed error classification (DNS, TLS, connect, timeout). Remove `reqwest` and `tokio` from `Cargo.toml`
- [x] Remove the `sync_wrapper` exception from `deny.toml`, ban the crate, and re-run `cargo deny check`. (done: the exception is removed, the `ryu` exception is removed too since it was unused, `cargo deny check` passes. `sync_wrapper` is not in the tree, so no ban was needed) Confirm no other Apache-only or `AND`-Apache crates remain, so the `ring` and `sync_wrapper` exceptions are both gone (`ryu` stays: it is an OR licence)
- [ ] Check the licences of each RustCrypto crate needed (must pass the same allowlist)
- [ ] Survey existing providers (e.g. `rustls-rustcrypto`) for maturity, licence and gaps; reuse or contribute before building
- [ ] Create the new crate (dual MIT/Apache-2.0, TPT Solutions), with a `rustls` provider as its first deliverable
- [ ] Test vectors: Wycheproof, RFC vectors and rustls's own interop tests
- [ ] Constant-time review of anything touching secret data
- [ ] Plan an independent security review before recommending it for production use
- [ ] Benchmark against ring (handshake time, throughput, binary size)
- [ ] Swap into `tpt-cli`, drop the `ring` exception from `deny.toml` and update the README

## Phase 10: Review fixes, adoption tooling and extras (post-review, 2026-11-12)
Out of scope here: Phase 9 (custom TLS/crypto) and crates.io publishing (blocked by the git dependency on `tpt-crypto-rustls`).

### 10a: Fixes
- [x] CI: `release-build` job uses `target/release/tpt`; the binary is `tptcli` (`ci.yml:62-64`, plus the step title)
- [x] Wire up the dead `--yes` flag (`src/cli.rs:30`): read it in `main.rs`/`CliOverrides` and give a command a real use of `prompts::confirm_action`
- [x] Delete `SECURITY.md` (decided) and remove all references: `.github/ISSUE_TEMPLATE/config.yml`, `README.md` (lines ~180, 207-208), `CONTRIBUTING.md:22`, `CHANGELOG.md:17`, the `Cargo.toml:36` comment. Keep the unaudited-TLS warning in the README itself
- [x] README: `tpt` -> `tptcli` in the examples (lines ~32, 34)
- [ ] CHANGELOG: move the reqwest/ring/rename entries to the right section and fix stale wording in 0.1.0 (still says `ring` and `tpt`)
- [ ] `THIRD-PARTY-LICENSES.md`: drop the removed `sync_wrapper`/`ryu` exceptions from the intro and regenerate (the list is Windows-only; note it or generate per platform)
- [ ] Refresh stale notes in this file: tick line 171 (`sync_wrapper`, done by line 174); update the decisions list at the top (reqwest, `ring`, `tpt` binary name, `dirs`, `tpt-solutions/tpt-cli`)
- [x] Config path: honour `XDG_CONFIG_HOME` and `%APPDATA%`, keep `~/.config/tpt/config.toml` as a fallback (`src/config.rs:149`)
- [x] CI: add `cargo doc` with `RUSTDOCFLAGS=-D warnings` and a clippy run with `--no-default-features`
- [ ] Doctests for `net::ping`, `Output::data/error/info/warn` and the logger wrappers
- [ ] Missing tests: `prompts.rs`, `logger.rs`, `NO_COLOR`/TTY behaviour, Windows config path, a real HTTPS integration test
- [x] Verify the CI trigger branch (`master`) matches the repo default branch

### 10b: Adoption tooling
- [x] `tptcli completions <shell>` (clap_complete: bash, zsh, fish, powershell, elvish); check the size impact against the size-first release profile
- [x] `tptcli man` (clap_mangen)
- [x] `tptcli init`: write a commented starter config (global, or `--local` for `./.tpt.toml`); confirm before overwrite, honour `--yes`
- [x] `tptcli config {path,list,get,set,edit}` and a global `--config <path>` flag; `config list` shows each value's source (default, file, env, flag)
- [x] `tptcli doctor`: version, config files and parse status, resolved values with sources, DNS and TLS root-store check, optional connectivity test; works with `--output json`
- [x] Release workflow on tag: linux/macos/windows builds (x86_64 and aarch64 where possible), archives, `SHA256SUMS`, release notes from the CHANGELOG. Evaluate `cargo-dist` first (decision needed: cargo-dist or a hand-written workflow)
- [x] `[package.metadata.binstall]` for `cargo binstall`; Cargo.toml metadata (repository, keywords, categories)
- [x] `install.sh` and `install.ps1` (checksum-verified)
- [ ] Dockerfile (not started)
- [ ] After the first release: Scoop and winget manifests, Homebrew formula
- [ ] `cargo-fuzz` target for `net::parse_reply` / `read_head`; coverage job in CI

### 10c: Extras (agent- and human-friendly)
- [x] `schema --format mcp`: MCP-style tool definitions, one per leaf command, with JSON Schema `inputSchema`. Done as one format (`mcp`), not separate `json-schema` output. Snapshot updated deliberately
- [x] Minimal field selection for JSON output (`--select data.status`, dot-path, no heavy deps)
- [x] Progress line (documented spinner was never built): shown for `ping` on a human terminal only, off for JSON, pipes and `--quiet`. Not animated, so it is a status line rather than a spinner
- [x] `ping`: `--retries` with backoff (transient failures only), `--header`, timing breakdown (dns, connect, tls, ttfb) and `attempts` in the JSON output
- [x] `ping --count`/`--interval`: every result is printed; the exit code is non-zero if any ping failed (the code of the last failure). The JSON data has `sent`, `received` and a `results` array, and no second error envelope is printed
- [x] `ping` proxy support from `HTTP_PROXY`/`HTTPS_PROXY`/`NO_PROXY` (lower case too): plain http sends the absolute URL, https uses a CONNECT tunnel. Limits: `http://` proxy URLs only, no proxy credentials
- [x] `--debug` flag prints the Rust debug form of the error
- [ ] Error hints with dynamic context (hints are still static strings)
- [x] Error-code table exposed in `schema` (`error_codes`), kept in sync with `TptError::code` by a test
- [x] Scaffold for consumers: `templates/cli`, a `cargo-generate` template (chosen over `tptcli new` so templating stays out of the size-first binary). Rendered by hand and built against the local library; not yet tested through `cargo generate` itself

## Backlog / not in scope (decide later)
- [ ] Publish to crates.io (check the `tpt-cli` name is available)
- [ ] Prebuilt binaries on GitHub Releases (cross-compiled, checksums)
- [ ] Plugin / subcommand extension system for other TPT repos
- [ ] Shell completions and man pages
- [ ] Docs website
