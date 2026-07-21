# tools/

Maintainer scripts. Run with [`uv`](https://docs.astral.sh/uv/).

| Script | Purpose |
| --- | --- |
| `compat_inventory.py` | Compatibility matrices under `docs/compatibility/` (`--check` in CI) |
| `compare_searxng.py` | Fixture / live API comparison vs SearXNG (`fixtures` in CI) |
| `fetch_tracker_patterns.py` | Refresh ClearURLs rules → `zoeken-data/data/tracker_patterns.json` |
| `sync_versions.py` | Sync `package.json` + lockfile zoeken-* versions to Cargo.toml |

```sh
uv run --no-project --python 3.13 tools/compat_inventory.py --check
uv run --no-project --python 3.13 tools/compare_searxng.py fixtures
uv run --no-project --python 3.13 tools/fetch_tracker_patterns.py
uv run --no-project --python 3.13 tools/sync_versions.py --dry-run
```

`fetch_tracker_patterns.py` writes `zoeken/zoeken-data/data/tracker_patterns.json`
(ClearURLs snapshot). Not run on server boot — refresh manually, then rebuild.
