#!/usr/bin/env bash
# Pre-tag / release gate: fmt, clippy, client biome, and version sync check.
# Usage: ./tools/pre_release.sh
# Optional: SKIP_CLIPPY=1 to skip clippy (fmt + client + sync still run).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "==> cargo fmt --check"
cargo fmt --all -- --check

if [[ "${SKIP_CLIPPY:-0}" != "1" ]]; then
  echo "==> cargo clippy (workspace, -D warnings)"
  cargo clippy --workspace --all-targets --locked -- -D warnings
fi

echo "==> client biome check"
if command -v biome >/dev/null 2>&1; then
  (cd zoeken-client && biome check .)
else
  (cd zoeken-client && bun run check)
fi

echo "==> sync_versions --check"
chmod +x tools/sync_versions.sh
./tools/sync_versions.sh --check

echo "pre_release: ok"
