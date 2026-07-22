# Zoeken

SearXNG-compatible metasearch engine: Rust backend + React SPA.

**Author:** [Greenstorm](https://github.com/Greenstorm5417)  
**Repository:** https://github.com/Greenstorm5417/zoeken  
**License:** [AGPL-3.0-or-later](LICENSE)

## Quick start

```sh
# Build SPA → zoeken/zoeken-server/assets
cd zoeken-client && bun install && bun run build && cd ..

# Release binary (set CARGO_TARGET_DIR on Windows if needed)
cargo build --release --bin zoeken-server

APP_ASSETS_DIR=zoeken/zoeken-server/assets ./target/release/zoeken-server
# → http://127.0.0.1:8888
```

Or `make build` / `make package` on Unix.

## Docs

| Doc | Contents |
| --- | --- |
| [`CHANGELOG.md`](CHANGELOG.md) | Release notes |
| [`SECURITY.md`](SECURITY.md) | Vulnerability reporting |
| [`default.config.yml`](default.config.yml) | Every configuration option at its typed default |
| [`docs/settings.yml.example`](docs/settings.yml.example) | Full YAML settings reference |
| [`docs/deployment.md`](docs/deployment.md) | Build, deb, systemd, Docker, GHCR |
| [`docs/client-features.md`](docs/client-features.md) | SPA client features (former plugins) |
| [`docs/compatibility/scorecard.md`](docs/compatibility/scorecard.md) | Compatibility scorecard |
| [`docs/compatibility/intentional-differences.md`](docs/compatibility/intentional-differences.md) | Deliberate gaps |
| [`docs/security/audit.md`](docs/security/audit.md) | Security controls + residual risk |
| [`tools/README.md`](tools/README.md) | Maintainer inventory / compare tooling |

## Releases

Current version: **1.3.0**. Tagged versions (`vX.Y.Z`, matching `Cargo.toml` and
`zoeken-client/package.json`) publish:

- Debian packages (`amd64` / `arm64`) with systemd unit + `/usr/share/zoeken/assets`
- Nix package archives (`x86_64-linux` / `aarch64-linux`) consumed by
  `github:Greenstorm5417/nixos-pkgs#zoeken`
- Multi-arch Docker image on GHCR: `ghcr.io/greenstorm5417/zoeken`

Dependency updates for Cargo, the SPA (`zoeken-client`), GitHub Actions, and
Docker base images are opened weekly by Dependabot (`.github/dependabot.yml`).

See [`docs/deployment.md`](docs/deployment.md) and [`CHANGELOG.md`](CHANGELOG.md).

## Compatibility checks

```sh
uv run --no-project --python 3.13 tools/compat_inventory.py --check
uv run --no-project --python 3.13 tools/compare_searxng.py fixtures
# Live (optional):
uv run --no-project --python 3.13 tools/compare_searxng.py live \
  --zoeken http://127.0.0.1:8888 --searxng http://127.0.0.1:8080
```

## License

[GNU Affero General Public License v3.0 or later](LICENSE) (AGPL-3.0-or-later).

Copyright (c) 2024–2026 Greenstorm.
