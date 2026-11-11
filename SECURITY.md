# Security Policy

## Supported versions

Until `1.0`, only the latest released version receives security fixes.

## Reporting a vulnerability

Please do not open a public issue for a security problem.

Report it privately to the TPT Solutions maintainers. The reporting address is to be added before the first public release, so until then, ask the maintainers for a private channel and do not post details publicly.

Include:

- The version or commit, and your platform
- Steps to reproduce, or a proof of concept
- The impact as you understand it

We aim to acknowledge a report within 5 working days and to share a plan within 14 days.
We will credit you in the advisory unless you ask us not to.

## Scope

In scope: the `tptcli` binary and the `tpt-cli` library. This covers config loading, network
requests (`tptcli ping`), TLS verification, and output handling.

Out of scope: vulnerabilities in dependencies that have no effect on tpt-cli. Report
those upstream. If a dependency affects tpt-cli, tell us as well.

## Design notes

- TLS uses rustls with the operating system's certificate store (`rustls-native-certs`).
  Certificate checks are not disabled.
- TLS cryptography comes from `tpt-crypto-rustls`, a pure-Rust provider built on `tpt-crypto`.
  **Neither has been independently audited or formally verified.** The risk is accepted for
  now, as a deliberate decision to drop `ring`. Do not use `tptcli ping` to protect sensitive traffic
  until the audit is complete. Report any suspected flaw in the TLS path as a security issue.
- The core library has no C dependencies and no OpenSSL. Neither build contains `ring`.
