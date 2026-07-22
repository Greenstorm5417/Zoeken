#!/usr/bin/env bash
# Sync repo metadata versions to [workspace.package].version in Cargo.toml.
#
# Source of truth: root Cargo.toml `[workspace.package].version`.
# Updates:
#   - zoeken-client/package.json
#   - Cargo.lock packages named zoeken-* (workspace members)
#   - Dockerfile / Dockerfile.runtime `ARG VERSION=` defaults
#   - docker-compose.yml `${VERSION:-…}` build-arg default
#
# Debian/Nix packaging already substitute version at build time.
# Pure bash + awk/sed — no Python (works on NixOS without stub-ld CPython).
#
# Examples:
#   ./tools/sync_versions.sh --dry-run
#   ./tools/sync_versions.sh
#   ./tools/sync_versions.sh --bump 1.2.0
#   ./tools/sync_versions.sh --check
#   make sync-versions
#   make sync-versions BUMP=1.2.0

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO_TOML="${ROOT}/Cargo.toml"
CLIENT_PKG="${ROOT}/zoeken-client/package.json"
CARGO_LOCK="${ROOT}/Cargo.lock"
COMPOSE="${ROOT}/docker-compose.yml"

BUMP=""
DRY_RUN=0
CHECK=0

usage() {
  sed -n '2,21p' "$0" | sed 's/^# \{0,1\}//'
  exit "${1:-0}"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --bump)
      [[ $# -ge 2 ]] || { echo "error: --bump needs a semver" >&2; exit 2; }
      BUMP="$2"
      shift 2
      ;;
    --dry-run) DRY_RUN=1; shift ;;
    --check) CHECK=1; shift ;;
    -h|--help) usage 0 ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage 2
      ;;
  esac
done

if [[ -n "${BUMP}" && ! "${BUMP}" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$ ]]; then
  echo "error: invalid semver for --bump: ${BUMP}" >&2
  exit 1
fi

read_workspace_version() {
  local ver
  ver="$(
    sed -n '/^\[workspace\.package\]/,/^\[/{s/^version *= *"\([^"]*\)".*/\1/p;}' \
      "${CARGO_TOML}" | head -n1
  )"
  if [[ -z "${ver}" ]]; then
    echo "error: could not read [workspace.package].version from Cargo.toml" >&2
    exit 1
  fi
  printf '%s' "${ver}"
}

PLANNED=()

# Queue a file rewrite. Writes immediately unless --dry-run/--check.
# `content` comes from $(awk|cat); command substitution strips trailing newlines,
# so we always write with exactly one trailing newline.
plan_write() {
  local path="$1"
  local summary="$2"
  local content="$3"
  local rel="${path#"${ROOT}/"}"
  if [[ -f "${path}" ]] && cmp -s <(printf '%s\n' "${content}") "${path}"; then
    return 0
  fi
  PLANNED+=("${rel}: ${summary}")
  if [[ "${CHECK}" -eq 1 || "${DRY_RUN}" -eq 1 ]]; then
    return 0
  fi
  printf '%s\n' "${content}" >"${path}"
}

bump_cargo_toml() {
  local version="$1"
  local old new
  old="$(read_workspace_version)"
  if [[ "${old}" == "${version}" ]]; then
    return 0
  fi
  new="$(
    awk -v ver="${version}" '
      /^\[workspace\.package\]/ { in_ws = 1 }
      in_ws && /^\[/ && $0 !~ /^\[workspace\.package\]/ { in_ws = 0 }
      in_ws && /^version[[:space:]]*=/ && !done {
        sub(/"[^"]*"/, "\"" ver "\"")
        done = 1
      }
      { print }
    ' "${CARGO_TOML}"
  )"
  plan_write "${CARGO_TOML}" "${old} -> ${version}" "${new}"
}

sync_package_json() {
  local version="$1"
  local old new
  old="$(
    sed -n 's/^[[:space:]]*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
      "${CLIENT_PKG}" | head -n1
  )"
  if [[ -z "${old}" ]]; then
    echo "error: could not read version from zoeken-client/package.json" >&2
    exit 1
  fi
  if [[ "${old}" == "${version}" ]]; then
    return 0
  fi
  new="$(
    awk -v ver="${version}" '
      !done && match($0, /^([[:space:]]*"version"[[:space:]]*:[[:space:]]*")[^"]*(".*)$/, a) {
        print a[1] ver a[2]
        done = 1
        next
      }
      { print }
    ' "${CLIENT_PKG}"
  )"
  plan_write "${CLIENT_PKG}" "${old} -> ${version}" "${new}"
}

sync_cargo_lock() {
  local version="$1"
  [[ -f "${CARGO_LOCK}" ]] || return 0

  local chg tmp new
  chg="$(mktemp)"
  tmp="$(mktemp)"

  awk -v ver="${version}" -v chg="${chg}" '
    {
      if ($0 == "[[package]]") {
        pkg_line = $0
        if ((getline name_line) <= 0) {
          print pkg_line
          next
        }
        if ((getline ver_line) <= 0) {
          print pkg_line
          print name_line
          next
        }
        if (name_line ~ /^name = "zoeken-/ && ver_line ~ /^version = "/) {
          old = ver_line
          sub(/^version = "/, "", old)
          sub(/"$/, "", old)
          if (match(name_line, /^name = "(zoeken-[^"]+)"/, m) && old != ver) {
            printf "%s\t%s\n", m[1], old >> chg
            ver_line = "version = \"" ver "\""
          }
        }
        print pkg_line
        print name_line
        print ver_line
        next
      }
      print
    }
  ' "${CARGO_LOCK}" >"${tmp}"

  if [[ ! -s "${chg}" ]]; then
    rm -f "${chg}" "${tmp}"
    return 0
  fi

  local changes=0 sample_old="" names=()
  while IFS=$'\t' read -r name old; do
    names+=("${name}")
    sample_old="${old}"
    changes=$((changes + 1))
  done <"${chg}"
  rm -f "${chg}"

  new="$(cat "${tmp}")"
  rm -f "${tmp}"
  local namelist=""
  local i
  for i in "${!names[@]}"; do
    if [[ "${i}" -gt 0 ]]; then
      namelist+=", "
    fi
    namelist+="${names[$i]}"
  done
  plan_write \
    "${CARGO_LOCK}" \
    "${changes} pkgs ${sample_old} -> ${version} (${namelist})" \
    "${new}"
}

sync_dockerfile_arg() {
  local path="$1"
  local version="$2"
  local old new
  old="$(sed -n 's/^ARG VERSION=//p' "${path}" | head -n1)"
  if [[ -z "${old}" ]]; then
    echo "error: could not find ARG VERSION= in $(basename "${path}")" >&2
    exit 1
  fi
  if [[ "${old}" == "${version}" ]]; then
    return 0
  fi
  new="$(
    awk -v ver="${version}" '
      !done && /^ARG VERSION=/ {
        print "ARG VERSION=" ver
        done = 1
        next
      }
      { print }
    ' "${path}"
  )"
  plan_write "${path}" "${old} -> ${version}" "${new}"
}

sync_compose_default() {
  local version="$1"
  local old new
  old="$(
    sed -n 's/.*VERSION: ${VERSION:-\([^}]*\)}.*/\1/p' "${COMPOSE}" | head -n1
  )"
  if [[ -z "${old}" ]]; then
    echo "error: could not find VERSION: \${VERSION:-…} in docker-compose.yml" >&2
    exit 1
  fi
  if [[ "${old}" == "${version}" ]]; then
    return 0
  fi
  new="$(
    awk -v ver="${version}" '
      !done && match($0, /^([[:space:]]*VERSION: \$\{VERSION:-)[^}]+(\}[[:space:]]*)$/, a) {
        print a[1] ver a[2]
        done = 1
        next
      }
      { print }
    ' "${COMPOSE}"
  )"
  plan_write "${COMPOSE}" "${old} -> ${version}" "${new}"
}

# --- main ---

if [[ -n "${BUMP}" ]]; then
  bump_cargo_toml "${BUMP}"
  VERSION="${BUMP}"
else
  VERSION="$(read_workspace_version)"
fi

sync_package_json "${VERSION}"
sync_cargo_lock "${VERSION}"
sync_dockerfile_arg "${ROOT}/Dockerfile" "${VERSION}"
sync_dockerfile_arg "${ROOT}/Dockerfile.runtime" "${VERSION}"
sync_compose_default "${VERSION}"

if [[ ${#PLANNED[@]} -eq 0 ]]; then
  echo "ok: already synced to ${VERSION}"
  exit 0
fi

echo "sync to ${VERSION}:"
for line in "${PLANNED[@]}"; do
  echo "  ${line}"
done

if [[ "${CHECK}" -eq 1 ]]; then
  exit 1
fi
if [[ "${DRY_RUN}" -eq 1 ]]; then
  exit 0
fi

echo "wrote updates"
exit 0
