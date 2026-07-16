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
| `APP_DATA_DIR` | Optional JSON data overrides (defaults are precompiled) |
| `APP_REDIS_URL` / `APP_VALKEY_URL` | Optional remote KV URL |
| `APP_LOG_LEVEL` | Tracing filter (`info`, `debug`, …) |
| `APP_METRICS_ENABLED` | Expose `/metrics` when true |
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
| `/usr/share/zoeken/plugins/` | bundled Lua plugins |
| `/usr/share/doc/zoeken/LICENSE` | AGPL-3.0-or-later full text |
| `/usr/share/doc/zoeken/copyright` | Debian copyright file |
| `/usr/share/doc/zoeken/changelog.Debian.gz` | Debian changelog |
| `/usr/share/doc/zoeken/settings.yml.example` | copy of the full settings reference |
| `/usr/share/doc/zoeken/limiter.toml.example` | copy of the limiter reference |
| `/etc/zoeken/settings.yml` | **main YAML config** (conffile; edit this) |
| `/etc/zoeken/limiter.toml` | limiter / botdetect TOML (conffile) |
| `/etc/default/zoeken` | `APP_*` env for systemd |
| `/lib/systemd/system/zoeken.service` | unit (`zoeken` user) |
| `/var/lib/zoeken` | writable data dir (`APP_DATA_DIR`) |

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
`/var/lib/zoeken` and sets `APP_DATA_DIR` for optional on-disk data overrides.

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
5. **Probes**: liveness `/healthz`, readiness `/readyz` (returns not-ready while draining).
6. **Image proxy**: leave off unless you need it; when on, URLs stay HMAC-gated and redirects are not followed.
7. **Metrics**: `/metrics` and `/stats` are unauthenticated — restrict at the reverse proxy if the instance is public.
8. Read [`docs/security/audit.md`](security/audit.md) before go-live.

## Reverse proxy

Terminate TLS at nginx/Caddy. Trust only the proxy CIDRs via
`deployment.trusted_proxies` and/or `trusted_proxies` in `limiter.toml` so
`X-Forwarded-For` / scheme forwarding is honored. Do not trust the open
internet as a proxy. Optionally block `/metrics` and `/stats` from the public.

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

1. Bump `[workspace.package].version` in `Cargo.toml` and `zoeken-client/package.json`
   to the same semver (e.g. `1.0.0`). Update `CHANGELOG.md`.
2. Commit, then tag and push: `git tag v1.0.0 && git push origin v1.0.0`.
3. GitHub Actions verifies Cargo + client versions match the tag, builds `.deb`s
   on native amd64/arm64 runners, pushes GHCR via `Dockerfile.runtime`, and opens
   the GitHub Release.
