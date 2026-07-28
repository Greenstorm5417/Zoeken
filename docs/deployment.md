# Deployment

Version tags (`vX.Y.Z`) must match `[workspace.package].version` in
`Cargo.toml` and `zoeken-client/package.json`. Pushing a matching tag runs the
release workflow: Debian packages (amd64/arm64), a multi-arch GHCR image, and a
GitHub Release.

## Build

```sh
# Linux/macOS
make build          # client assets + release zoeken-server
make package        # also copies assets beside target/release/
make deb            # .deb for the host arch (needs dpkg-deb)
make docker         # local image tagged zoeken:local

# Windows (PowerShell)
cd zoeken-client; bun install; bun run build; cd ..
cargo build --release --locked --bin zoeken-server
```

Frontend builds into `zoeken/zoeken-server/assets`. The binary does **not**
embed SPA files; ship `./assets` next to the binary (or set `APP_ASSETS_DIR`).

## Run (from source)

```sh
# Loopback default — secret key optional (dev only)
./target/release/zoeken-server

# Public bind — strong secret required (≥16 chars, not a placeholder)
APP_BIND_ADDRESS=0.0.0.0 \
APP_SECRET_KEY="$(openssl rand -hex 24)" \
APP_PUBLIC_INSTANCE=true \
./target/release/zoeken-server
```

Useful env vars (see `zoeken-settings`):

| Env | Purpose |
| --- | --- |
| `APP_BIND_ADDRESS` | Listen address (`127.0.0.1` default) |
| `APP_PORT` | Port (`8888` default) |
| `APP_SECRET_KEY` | HMAC + prefs signing; required (≥16 chars) off-loopback |
| `APP_PUBLIC_INSTANCE` | Force-enable limiter on non-loopback binds |
| `APP_LIMITER` | Explicit limiter on/off |
| `APP_BASE_URL` | Public base URL for absolute links |
| `APP_IMAGE_PROXY` | Enable image proxy (`true`/`false`) |
| `APP_METHOD` | Default HTTP method for search forms |
| `APP_ASSETS_DIR` | SPA directory override |
| `APP_SETTINGS_PATH` | `settings.yml` path |
| `APP_DATA_DIR` | Optional JSON data directory (**full bundle only**, no merge with embedded defaults; missing files fail boot). Leave unset to use the precompiled bundle. |
| `APP_STORAGE_BACKEND` | `sqlite` (default) or `postgres` |
| `APP_SQLITE_PATH` | SQLite database path |
| `APP_SQLITE_MAX_CONNECTIONS` | SQLite pool size (default `4`) |
| `APP_POSTGRES_URL` | PostgreSQL connection URL (never logged) |
| `APP_LOG_LEVEL` | Tracing filter (`info`, `debug`, …) |
| `APP_METRICS_ENABLED` | Expose `/metrics` when true |
| `APP_DISABLE_UI` | Skip SPA `index.html` boot check (JSON-only; also `server.disable_ui`) |
| `APP_DEBUG` | `general.debug` |

## Configuration

Zoeken uses **YAML** for the main app config and **TOML** for the rate
limiter / botdetection:

| File | Format | Role |
| --- | --- | --- |
| `settings.yml` | YAML | Server, search, engines, plugins, outgoing, deployment, … |
| `limiter.toml` | TOML | Trusted proxies, IP lists, token-bucket rate limits, heuristics |

Full commented reference (every typed option):
[`docs/settings.yml.example`](settings.yml.example)
(same file packaged as `/etc/zoeken/settings.yml` and
`/usr/share/doc/zoeken/settings.yml.example`).

Load order: built-in defaults → settings file (`APP_SETTINGS_PATH`) →
`APP_*` env overrides. The Debian unit sets
`APP_SETTINGS_PATH=/etc/zoeken/settings.yml`.

## Debian package + systemd

Release assets are named `zoeken_<version>_<amd64|arm64>.deb`.

```sh
sudo apt install ./zoeken_1.0.0_amd64.deb
sudoedit /etc/default/zoeken    # set APP_SECRET_KEY before public bind
sudoedit /etc/zoeken/settings.yml   # full YAML config (all options)
sudoedit /etc/zoeken/limiter.toml   # rate limits + trusted_proxies
sudo systemctl start zoeken
sudo systemctl status zoeken
```

| Path | Contents |
| --- | --- |
| `/usr/bin/zoeken-server` | server binary |
| `/usr/share/zoeken/assets/` | SPA |
| `/usr/share/doc/zoeken/LICENSE` | AGPL-3.0-or-later full text |
| `/usr/share/doc/zoeken/copyright` | Debian copyright file |
| `/usr/share/doc/zoeken/changelog.Debian.gz` | Debian changelog |
| `/usr/share/doc/zoeken/settings.yml.example` | copy of the full settings reference |
| `/usr/share/doc/zoeken/limiter.toml.example` | copy of the limiter reference |
| `/etc/zoeken/settings.yml` | **main YAML config** (conffile; edit this) |
| `/etc/zoeken/limiter.toml` | limiter / botdetect TOML (conffile) |
| `/etc/default/zoeken` | `APP_*` env for systemd |
| `/lib/systemd/system/zoeken.service` | unit (`zoeken` user) |
| `/var/lib/zoeken` | writable state dir (set `APP_DATA_DIR` only with a full JSON bundle) |

The unit enables on install but does **not** start automatically. Default bind
is loopback; set `APP_BIND_ADDRESS=0.0.0.0` and a strong `APP_SECRET_KEY` for
a public instance, then `systemctl restart zoeken`.

Local package build (amd64 host):

```sh
make deb-amd64
make deb-arm64   # native aarch64 host (release CI uses ubuntu-24.04-arm)
```

## Docker

`Dockerfile` builds from source. Release tags use `Dockerfile.runtime` with
prebuilt binaries from the shared release-binary job (deb packaging and Docker
image push run in parallel after that).

Image runs as non-root with `/app/zoeken-server`, `/app/assets`, and the AGPL
license under `/usr/share/licenses/zoeken/` (and `/app/LICENSE`). Default bind
is loopback — set `APP_BIND_ADDRESS=0.0.0.0` for published ports.

### GHCR (release tags)

```sh
docker pull ghcr.io/greenstorm5417/zoeken:latest
# or a specific version: ghcr.io/greenstorm5417/zoeken:1.0.0

docker run --rm \
  -e APP_BIND_ADDRESS=0.0.0.0 \
  -e APP_SECRET_KEY="$(openssl rand -hex 24)" \
  -e APP_PUBLIC_INSTANCE=true \
  -p 8888:8888 \
  ghcr.io/greenstorm5417/zoeken:latest
```

Multi-arch images: `linux/amd64` and `linux/arm64`.

### Compose

```sh
cp .env.example .env
# set APP_SECRET_KEY=$(openssl rand -hex 24)  — empty/placeholder values are rejected
docker compose up -d --build
```

Image `HEALTHCHECK` curls `http://127.0.0.1:8888/healthz`. Compose mounts
`/var/lib/zoeken` for writable state; leave `APP_DATA_DIR` unset so the
precompiled data bundle is used (set it only when shipping a full on-disk
JSON bundle).

SQLite is the default and supports one Zoeken process. For coordinated
multi-replica development, start the optional PostgreSQL profile and point
every replica at the same database:

```sh
# .env must set POSTGRES_PASSWORD (replace change-me-before-production)
# and APP_STORAGE_BACKEND=postgres
docker compose --profile postgres up -d
```

Zoeken joins the compose network, waits for healthy postgres when the profile
is active, and uses `APP_POSTGRES_URL` (default
`postgres://zoeken:$POSTGRES_PASSWORD@postgres:5432/zoeken`). Set
`POSTGRES_PASSWORD` and use the matching URL outside local development.
Zoeken fails startup when the selected database cannot connect or migrate;
after startup, `/readyz` becomes unhealthy and uncached outbound requests fail
closed if storage coordination is unavailable.

## Nix / NixOS

Release tags ship `zoeken_<version>_<x86_64-linux|aarch64-linux>.tar.gz`. The
Zoeken flake wraps those archives (no local Rust/SPA build). **Do not put a
version in the flake URL** — depend on `github:Greenstorm5417/Zoeken` and let
**your** `flake.lock` pin the revision (and thus the prebuilt pointed at by
`packaging/nix/generated.nix` on that rev). Release binaries include **SQLite
and Postgres** (default Cargo features).

```sh
nix run github:Greenstorm5417/Zoeken
nix profile install github:Greenstorm5417/Zoeken
# rolling auto-updater mirror:
nix run github:Greenstorm5417/nixos-pkgs#zoeken
```

### NixOS system install

```nix
# flake.nix (excerpt)
{
  inputs.zoeken.url = "github:Greenstorm5417/Zoeken";

  outputs = { nixpkgs, zoeken, ... }: {
    nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux"; # or aarch64-linux
      modules = [{
        environment.systemPackages = [ zoeken.packages.x86_64-linux.zoeken ];
        # Optional systemd unit — set APP_SECRET_KEY via EnvironmentFile=
        systemd.services.zoeken = {
          description = "Zoeken metasearch";
          after = [ "network-online.target" ];
          wantedBy = [ "multi-user.target" ];
          serviceConfig.ExecStart =
            "${zoeken.packages.x86_64-linux.zoeken}/bin/zoeken-server";
          serviceConfig.Restart = "on-failure";
        };
      }];
    };
  };
}
```

Then `nix flake lock` (or `nix flake update zoeken`) to refresh the pin.

Rolling mirror via [nixos-pkgs](https://github.com/Greenstorm5417/nixos-pkgs):

```nix
environment.systemPackages = [ nixos-pkgs.packages.${pkgs.system}.zoeken ];
```

After publishing a release, refresh in-repo prebuilt pointers (so `main`
consumers see the new binary; their locks still pin a git rev):

```sh
./tools/update_nix_generated.sh
```

Low-power / single-node **from-source** builds can omit PostgreSQL (release
artifacts keep both backends):

```sh
cargo build --release --locked --bin zoeken-server --no-default-features
```

## Production checklist

1. **Bind + secret**: `0.0.0.0` (or LAN IP) with a random `APP_SECRET_KEY` ≥16 chars
   (not a `change-me…` placeholder).
2. **Limiter**: `APP_PUBLIC_INSTANCE=true` (or `server.limiter: true` in
   `settings.yml`). Edit `/etc/zoeken/limiter.toml` for rate limits / IP lists;
   `settings.yml` → `limiter.file` points at it.
3. **TLS**: terminate at nginx/Caddy. Add the proxy CIDRs under
   `deployment.trusted_proxies` in `settings.yml` **and/or** in `limiter.toml`.
   Settings values are **unioned** into the limiter list at boot (loopback stays
   trusted by default).
4. **Assets**: ship `./assets` next to the binary, or use the deb/Docker paths above.
5. **Storage**: keep SQLite for one process. For multiple replicas, start the optional `postgres` Compose profile and set `APP_STORAGE_BACKEND=postgres` plus `APP_POSTGRES_URL`. Startup fails if connection or migration fails.
6. **Probes**: liveness `/healthz`, readiness `/readyz` (returns not-ready while draining or while operational storage is unavailable).
7. **Image proxy**: leave off unless you need it; when on, URLs stay HMAC-gated and redirects are not followed.
8. **Metrics**: set `general.open_metrics` to a password so `/metrics` and `/stats` require HTTP Basic auth; empty hides `/metrics` and denies `/stats` JSON (401).
9. Read [`docs/security/audit.md`](security/audit.md) before go-live.

## Reverse proxy

Terminate TLS at nginx/Caddy. Trust only the proxy CIDRs via
`deployment.trusted_proxies` and/or `trusted_proxies` in `limiter.toml` so
`X-Forwarded-For` / scheme forwarding is honored. Do not trust the open
internet as a proxy. Prefer `general.open_metrics`; optionally also block those paths at the edge.

Example (`settings.yml`):

```yaml
deployment:
  trusted_proxies:
    - 10.0.0.0/8      # Docker bridge / private LAN proxy
    - 172.16.0.0/12
```

## Migration from SearXNG

1. Start from a SearXNG `settings.yml`; unsupported keys are ignored/warned.
2. Engine names mostly match; see `docs/compatibility/engines.md` for skipped
   engines (API-key / command / deferred).
3. Themes/Jinja HTML are not served — use the Zoeken SPA against JSON APIs.
4. Preferences cookies remain mostly compatible; UI theme is unused (system
   light/dark only).
5. Review `docs/compatibility/intentional-differences.md` and
   `docs/security/audit.md` before going public.

## Cutting a release

1. Run the pre-tag gate: `./tools/pre_release.sh` (or
   `make pre-release`) — `cargo fmt --check`, clippy `-D warnings`, and
   `./tools/sync_versions.sh --check`.
2. Bump `[workspace.package].version` in `Cargo.toml` (source of truth), then sync
   dependents via **Actions → Sync versions** (or locally:
   `./tools/sync_versions.sh [--bump X.Y.Z]` / `make sync-versions BUMP=X.Y.Z`).
   The workflow commits `chore: sync package versions to X.Y.Z` when needed.
   Update `CHANGELOG.md`.
3. Commit remaining release notes if needed, then tag and push:
   `git tag v1.0.0 && git push origin v1.0.0`.
4. GitHub Actions verifies Cargo + client versions match the tag, builds `.deb`s
   on native amd64/arm64 runners, pushes GHCR via `Dockerfile.runtime`, and opens
   the GitHub Release. A follow-up job commits refreshed
   `packaging/nix/generated.nix` hashes to `main` when they change.
