# Changelog

All notable changes to Zoeken are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-07-16

First stable release: SearXNG-compatible metasearch (Rust backend + React SPA),
with Debian packages, systemd unit, and multi-arch Docker images on GHCR.

### Added

- `zoeken-server` HTTP API compatible with SearXNG search/config/stats/metrics routes
- React SPA (`zoeken-client`) served from on-disk assets
- ~248 ported engines; intentional skips documented in `docs/compatibility/`
- Lua plugin host and bundled plugins
- Rate limiting, secret-key gating, image/favicon proxy SSRF controls
- Debian packaging (`amd64` / `arm64`) with `zoeken.service`
- Multi-arch container image: `ghcr.io/greenstorm5417/zoeken`
- Deployment, security audit, and plugin docs

### Fixed

- `deployment.trusted_proxies` is unioned into the limiter trusted list (no longer
  ignored when bundled `limiter.toml` already lists loopback)
- Debian package ships `/etc/zoeken/limiter.toml` and systemd
  `ReadWritePaths=/var/lib/zoeken`
- Example compose secret rejected as weak; SPA Vite devtools plugin is
  development-only; CI builds/lints/tests the client on every PR

### Changed

- Ship a full commented YAML settings reference (`/etc/zoeken/settings.yml`,
  `docs/settings.yml.example`) covering every typed option; Debian/Docker also
  install Lua plugins under `/usr/share/zoeken/plugins`

### Compatibility notes

- Full SearXNG HTML/Jinja theme parity is **not** a goal; use the SPA + JSON APIs
- Command engines and several API-key / bespoke engines remain intentionally unsupported
- See `docs/compatibility/intentional-differences.md` and `docs/security/audit.md`

[1.0.0]: https://github.com/Greenstorm5417/zoeken/releases/tag/v1.0.0
