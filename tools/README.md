# tools/

Maintainer scripts. Python helpers use [`uv`](https://docs.astral.sh/uv/);
`sync_versions.sh` is pure bash (no Python — works on NixOS without stub-ld).

| Script | Purpose |
| --- | --- |
| `compat_inventory.py` | Compatibility matrices under `docs/compatibility/` (`--check` in CI) |
| `compare_searxng.py` | Fixture / live API comparison vs SearXNG (`fixtures` in CI) |
| `fetch_tracker_patterns.py` | Refresh ClearURLs rules → `zoeken-data/data/tracker_patterns.json` |
| `sync_versions.sh` | Sync package.json, lockfile zoeken-*, and Docker VERSION defaults to Cargo.toml |

```sh
uv run --no-project --python 3.13 tools/compat_inventory.py --check
uv run --no-project --python 3.13 tools/compare_searxng.py fixtures
uv run --no-project --python 3.13 tools/fetch_tracker_patterns.py
./tools/sync_versions.sh --dry-run
make sync-versions BUMP=1.4.0
```

`fetch_tracker_patterns.py` writes `zoeken/zoeken-data/data/tracker_patterns.json`
(ClearURLs snapshot for the SPA). Not run on boot — refresh manually, then
`cd zoeken-client && bun run sync-data` (also runs as part of `bun run build`).
