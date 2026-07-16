# Security Policy

## Supported versions

| Version | Supported |
| ------- | --------- |
| 1.x     | Yes       |
| < 1.0   | No        |

## Reporting a vulnerability

Please **do not** open a public GitHub issue for security-sensitive reports.

Prefer one of:

1. [GitHub private vulnerability reporting](https://github.com/Greenstorm5417/zoeken/security/advisories/new) for this repository (if enabled), or
2. Email the maintainer: **sdussinger1007@gmail.com** (also listed in `Cargo.toml`).

Include enough detail to reproduce (affected version/tag, config, request shape).
We will acknowledge receipt when possible and coordinate a fix before public disclosure.

## Operator guidance

Before exposing an instance publicly, follow:

- [`docs/security/audit.md`](docs/security/audit.md) — controls and residual risk
- [`docs/deployment.md`](docs/deployment.md) — production checklist (secret key, limiter, TLS, metrics)

Known residual risks (DNS rebinding after URL validation, unauthenticated `/metrics` and `/stats`) are documented in the security audit; mitigate at the reverse proxy / network edge when needed.
