#!/usr/bin/env bash
# Compat shim — implementation lives in sync_versions.sh (pure bash, no Python).
# Kept so old docs/muscle-memory (`tools/sync_versions.py …`) still work on NixOS
# where `uv run --python` downloads a CPython that stub-ld refuses to execute.
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/sync_versions.sh" "$@"
