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

Current version: **1.4.0**. Tagged versions (`vX.Y.Z`, matching `Cargo.toml` and
`zoeken-client/package.json`) publish:

- Debian packages (`amd64` / `arm64`) with systemd unit + `/usr/share/zoeken/assets`
- Nix package archives (`x86_64-linux` / `aarch64-linux`) for
  `nix run github:Greenstorm5417/Zoeken` (and the rolling
  `github:Greenstorm5417/nixos-pkgs#zoeken` mirror)
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

## Benchmarks

```sh
# Rust Criterion (HTML under target/criterion/)
cargo bench -p zoeken-search

# Flamegraph + cargo-bloat (needs cargo-flamegraph / perf / cargo-bloat on PATH)
./tools/perf_profile.sh
cargo flamegraph -p zoeken-search --bench aggregation
cargo bloat --release --bin zoeken-server --crates -n 40
# Smaller single-node binary (no PostgreSQL):
cargo build --release --bin zoeken-server --no-default-features

# Frontend (with zoeken-server on :8888; install lighthouse / sitespeed-io locally)
lighthouse "http://127.0.0.1:8888/search?q=rust" --chrome-flags="--headless --no-sandbox"
sitespeed-io "http://127.0.0.1:8888/" --browsertime.browser chrome
hyperfine 'curl -sf "http://127.0.0.1:8888/search?q=rust"'
```

### Nix package (prebuilt release)

```sh
nix run github:Greenstorm5417/Zoeken
nix profile install github:Greenstorm5417/Zoeken
# rolling mirror: nix run github:Greenstorm5417/nixos-pkgs#zoeken
```

On **NixOS**, add `zoeken.packages.<system>.zoeken` to `environment.systemPackages`
(see [docs/deployment.md](docs/deployment.md#nix--nixos)). Use a version-less flake
URL and pin with your `flake.lock`. Release binaries include SQLite and Postgres.

After each GitHub Release, refresh prebuilt pointers on `main` (optional if you
only consume a locked git rev):

```sh
./tools/update_nix_generated.sh
```

## License

[GNU Affero General Public License v3.0 or later](LICENSE) (AGPL-3.0-or-later).

Copyright (c) 2024–2026 Greenstorm.
