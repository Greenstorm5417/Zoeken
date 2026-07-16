#!/usr/bin/env bash
# Resolve package version (no leading v): VERSION env, then GITHUB_REF_NAME, then Cargo.toml.
set -euo pipefail

if [[ -n "${VERSION:-}" ]]; then
  echo "${VERSION#v}"
  exit 0
fi

if [[ -n "${GITHUB_REF_NAME:-}" && "${GITHUB_REF_NAME}" == v* ]]; then
  echo "${GITHUB_REF_NAME#v}"
  exit 0
fi

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
ver="$(
  sed -n '/^\[workspace\.package\]/,/^\[/{s/^version *= *"\([^"]*\)".*/\1/p;}' \
    "${ROOT}/Cargo.toml" | head -n1
)"
if [[ -z "${ver}" ]]; then
  echo "unable to determine package version" >&2
  exit 1
fi
echo "${ver}"
