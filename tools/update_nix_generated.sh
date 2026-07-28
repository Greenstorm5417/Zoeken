#!/usr/bin/env bash
# Update packaging/nix/generated.nix from a published GitHub release.
# Usage: ./tools/update_nix_generated.sh [VERSION]
# VERSION defaults to Cargo.toml workspace version (without leading v).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${1:-}"
if [[ -z "$VERSION" ]]; then
  VERSION="$(sed -n 's/^version = "\([0-9.]*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
fi
VERSION="${VERSION#v}"

OUT="$ROOT/packaging/nix/generated.nix"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

declare -A HASHES
for system in x86_64-linux aarch64-linux; do
  name="zoeken_${VERSION}_${system}.tar.gz"
  url="https://github.com/Greenstorm5417/Zoeken/releases/download/v${VERSION}/${name}"
  echo "fetching $url" >&2
  curl -fsSL "$url" -o "$TMP/$name"
  HASHES[$system]="$(nix hash file "$TMP/$name")"
done

{
  echo "# Auto-updated from GitHub release assets (see tools/update_nix_generated.sh)."
  echo "{"
  echo "  version = \"${VERSION}\";"
  echo "  sources = {"
  for system in x86_64-linux aarch64-linux; do
    echo "    \"${system}\" = {"
    echo "      url = \"https://github.com/Greenstorm5417/Zoeken/releases/download/v${VERSION}/zoeken_${VERSION}_${system}.tar.gz\";"
    echo "      hash = \"${HASHES[$system]}\";"
    echo "    };"
  done
  echo "  };"
  echo "}"
} >"$OUT"

echo "wrote $OUT" >&2
cat "$OUT"
