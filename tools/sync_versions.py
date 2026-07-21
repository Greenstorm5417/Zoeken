#!/usr/bin/env python3
"""Sync repo metadata versions to [workspace.package].version in Cargo.toml.

Source of truth: root Cargo.toml `[workspace.package].version`.
Updates:
  - zoeken-client/package.json
  - Cargo.lock packages named zoeken-* (workspace members)
  - Dockerfile / Dockerfile.runtime `ARG VERSION=` defaults
  - docker-compose.yml `${VERSION:-…}` build-arg default

Debian/Nix packaging already substitute version at build time.
Run before cutting a release (see .github/workflows/sync-versions.yml).

Examples:
  uv run --no-project --python 3.13 tools/sync_versions.py --dry-run
  uv run --no-project --python 3.13 tools/sync_versions.py
  uv run --no-project --python 3.13 tools/sync_versions.py --bump 1.2.0
  uv run --no-project --python 3.13 tools/sync_versions.py --check
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CARGO_TOML = ROOT / "Cargo.toml"
CLIENT_PKG = ROOT / "zoeken-client" / "package.json"
CARGO_LOCK = ROOT / "Cargo.lock"
DOCKERFILES = (ROOT / "Dockerfile", ROOT / "Dockerfile.runtime")
COMPOSE = ROOT / "docker-compose.yml"

SEMVER_RE = re.compile(r"^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$")
WORKSPACE_VERSION_RE = re.compile(
    r"(?ms)(^\[workspace\.package\]\s*\n(?:(?!^\[).*\n)*?^version\s*=\s*)\"([^\"]*)\""
)
PKG_JSON_VERSION_RE = re.compile(
    r'^(\s*"version"\s*:\s*")([^"]*)(")', re.MULTILINE
)
LOCK_PKG_RE = re.compile(
    r'(?ms)(\[\[package\]\]\nname = "(zoeken-[^"]+)"\nversion = ")([^"]*)(")'
)
DOCKER_ARG_RE = re.compile(r"^(ARG VERSION=)(\S+)\s*$", re.MULTILINE)
COMPOSE_VERSION_RE = re.compile(
    r"^(?P<prefix>\s*VERSION:\s*\$\{VERSION:-)(?P<old>[^}]+)(?P<suffix>\})\s*$",
    re.MULTILINE,
)


def read_workspace_version(text: str) -> str:
    m = WORKSPACE_VERSION_RE.search(text)
    if not m:
        raise SystemExit("could not read [workspace.package].version from Cargo.toml")
    return m.group(2)


def set_workspace_version(text: str, version: str) -> str:
    new, n = WORKSPACE_VERSION_RE.subn(rf'\1"{version}"', text, count=1)
    if n != 1:
        raise SystemExit("could not set [workspace.package].version in Cargo.toml")
    return new


def sync_package_json(text: str, version: str) -> tuple[str, str | None]:
    m = PKG_JSON_VERSION_RE.search(text)
    if not m:
        raise SystemExit("could not read version from zoeken-client/package.json")
    old = m.group(2)
    if old == version:
        return text, None
    return PKG_JSON_VERSION_RE.sub(rf"\g<1>{version}\g<3>", text, count=1), old


def sync_cargo_lock(text: str, version: str) -> tuple[str, list[tuple[str, str]]]:
    changes: list[tuple[str, str]] = []

    def repl(m: re.Match[str]) -> str:
        name, old = m.group(2), m.group(3)
        if old == version:
            return m.group(0)
        changes.append((name, old))
        return f"{m.group(1)}{version}{m.group(4)}"

    return LOCK_PKG_RE.sub(repl, text), changes


def sync_dockerfile_arg(text: str, version: str, path: Path) -> tuple[str, str | None]:
    m = DOCKER_ARG_RE.search(text)
    if not m:
        raise SystemExit(f"could not find ARG VERSION= in {path.name}")
    old = m.group(2)
    if old == version:
        return text, None
    return DOCKER_ARG_RE.sub(rf"\g<1>{version}", text, count=1), old


def sync_compose_default(text: str, version: str) -> tuple[str, str | None]:
    m = COMPOSE_VERSION_RE.search(text)
    if not m:
        raise SystemExit("could not find VERSION: ${VERSION:-…} in docker-compose.yml")
    old = m.group("old")
    if old == version:
        return text, None
    return COMPOSE_VERSION_RE.sub(
        lambda mm: f"{mm.group('prefix')}{version}{mm.group('suffix')}",
        text,
        count=1,
    ), old


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--bump",
        metavar="X.Y.Z",
        help="set Cargo.toml workspace version first, then sync dependents",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="print planned changes without writing files",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="exit 1 if any file would change (no writes)",
    )
    args = parser.parse_args()
    if args.bump and not SEMVER_RE.match(args.bump):
        raise SystemExit(f"invalid semver for --bump: {args.bump!r}")

    cargo_text = CARGO_TOML.read_text(encoding="utf-8")
    planned: list[tuple[Path, str, str]] = []  # path, summary, new content

    if args.bump:
        old = read_workspace_version(cargo_text)
        if old != args.bump:
            cargo_text = set_workspace_version(cargo_text, args.bump)
            planned.append((CARGO_TOML, f"{old} -> {args.bump}", cargo_text))

    version = read_workspace_version(cargo_text)

    pkg_text = CLIENT_PKG.read_text(encoding="utf-8")
    new_pkg, old_pkg = sync_package_json(pkg_text, version)
    if old_pkg is not None:
        planned.append((CLIENT_PKG, f"{old_pkg} -> {version}", new_pkg))

    if CARGO_LOCK.is_file():
        lock_text = CARGO_LOCK.read_text(encoding="utf-8")
        new_lock, lock_changes = sync_cargo_lock(lock_text, version)
        if lock_changes:
            names = ", ".join(n for n, _ in lock_changes)
            sample_old = lock_changes[0][1]
            planned.append(
                (CARGO_LOCK, f"{len(lock_changes)} pkgs {sample_old} -> {version} ({names})", new_lock)
            )

    for path in DOCKERFILES:
        text = path.read_text(encoding="utf-8")
        new_text, old = sync_dockerfile_arg(text, version, path)
        if old is not None:
            planned.append((path, f"{old} -> {version}", new_text))

    compose_text = COMPOSE.read_text(encoding="utf-8")
    new_compose, old_compose = sync_compose_default(compose_text, version)
    if old_compose is not None:
        planned.append((COMPOSE, f"{old_compose} -> {version}", new_compose))

    if not planned:
        print(f"ok: already synced to {version}")
        return 0

    print(f"sync to {version}:")
    for path, summary, _ in planned:
        print(f"  {path.relative_to(ROOT)}: {summary}")

    if args.check or args.dry_run:
        return 1 if args.check else 0

    for path, _, content in planned:
        path.write_text(content, encoding="utf-8", newline="\n")
    print("wrote updates")
    return 0


if __name__ == "__main__":
    sys.exit(main())
