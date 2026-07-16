# tools/

Maintainer scripts. Run with [`uv`](https://docs.astral.sh/uv/).

| Script | Purpose |
| --- | --- |
| `compat_inventory.py` | Compatibility matrices under `docs/compatibility/` (`--check` in CI) |
| `compare_searxng.py` | Fixture / live API comparison vs SearXNG (`fixtures` in CI) |
| `fetch_tracker_patterns.py` | Refresh ClearURLs rules → `zoeken-data/data/tracker_patterns.json` |

```sh
uv run --no-project --python 3.13 tools/compat_inventory.py --check
uv run --no-project --python 3.13 tools/compare_searxng.py fixtures
uv run --no-project --python 3.13 tools/fetch_tracker_patterns.py
```
