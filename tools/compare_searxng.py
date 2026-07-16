#!/usr/bin/env python3
"""Side-by-side SearXNG ↔ zoeken comparison harness.

Modes:
  live       Compare two running instances (zoeken + SearXNG).
  fixtures   Diff recorded JSON under tests/integration/compare/.
  record     Capture live responses into that fixtures tree.

Stdlib only. Examples:

  uv run --no-project --python 3.13 tools/compare_searxng.py fixtures
  uv run --no-project --python 3.13 tools/compare_searxng.py live \\
      --zoeken http://127.0.0.1:8888 --searxng http://127.0.0.1:8080
  uv run --no-project --python 3.13 tools/compare_searxng.py record \\
      --zoeken http://127.0.0.1:8888 --searxng http://127.0.0.1:8080
"""

from __future__ import annotations

import argparse
import json
import sys
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "tests" / "integration" / "compare"

# Paths that must exist on both sides for API compatibility.
COMPARE_PATHS = (
    "/config",
    "/stats",
    "/search?q=rust&format=json&pageno=1",
)

# Top-level keys required on zoeken JSON for the scored endpoints.
REQUIRED_KEYS = {
    "/config": {"categories", "engines", "autocomplete", "doi_resolvers"},
    "/stats": {"engines"},
    "/search?q=rust&format=json&pageno=1": {
        "query",
        "results",
        "answers",
        "corrections",
        "infoboxes",
        "suggestions",
        "unresponsive_engines",
    },
}


def fetch_json(base: str, path: str, timeout: float = 30.0) -> tuple[int, Any]:
    url = base.rstrip("/") + path
    req = urllib.request.Request(url, headers={"Accept": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            status = getattr(resp, "status", 200)
            body = resp.read().decode("utf-8")
            return status, json.loads(body)
    except urllib.error.HTTPError as exc:
        body = exc.read().decode("utf-8", errors="replace")
        try:
            return exc.code, json.loads(body)
        except json.JSONDecodeError:
            return exc.code, {"_raw": body}
    except Exception as exc:  # noqa: BLE001 — surface any transport failure
        raise SystemExit(f"request failed for {url}: {exc}") from exc


def fixture_name(path: str) -> str:
    return (
        path.lstrip("/")
        .replace("?", "__")
        .replace("&", "_")
        .replace("=", "-")
        .replace("/", "_")
        + ".json"
    )


def write_fixture(side: str, path: str, payload: Any) -> Path:
    out = FIXTURES / side
    out.mkdir(parents=True, exist_ok=True)
    target = out / fixture_name(path)
    target.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return target


def load_fixture(side: str, path: str) -> Any:
    target = FIXTURES / side / fixture_name(path)
    if not target.exists():
        raise SystemExit(f"missing fixture: {target}")
    return json.loads(target.read_text(encoding="utf-8"))


def top_keys(value: Any) -> set[str]:
    if isinstance(value, dict):
        return set(value.keys())
    return set()


def compare_keys(label: str, left: Any, right: Any) -> list[str]:
    issues: list[str] = []
    lk, rk = top_keys(left), top_keys(right)
    only_left = sorted(lk - rk)
    only_right = sorted(rk - lk)
    if only_left:
        issues.append(f"{label}: only in zoeken: {', '.join(only_left)}")
    if only_right:
        issues.append(f"{label}: only in searxng: {', '.join(only_right)}")
    return issues


def check_required(label: str, payload: Any, required: set[str]) -> list[str]:
    keys = top_keys(payload)
    missing = sorted(required - keys)
    if missing:
        return [f"{label}: missing required keys: {', '.join(missing)}"]
    return []


def run_live(zoeken: str, searxng: str) -> int:
    issues: list[str] = []
    for path in COMPARE_PATHS:
        zs, zj = fetch_json(zoeken, path)
        ss, sj = fetch_json(searxng, path)
        if zs != 200:
            issues.append(f"{path}: zoeken status {zs}")
        if ss != 200:
            issues.append(f"{path}: searxng status {ss}")
        issues.extend(compare_keys(path, zj, sj))
        required = REQUIRED_KEYS.get(path, set())
        issues.extend(check_required(f"{path} (zoeken)", zj, required))
    return report(issues)


def run_fixtures() -> int:
    issues: list[str] = []
    notes: list[str] = []
    for path in COMPARE_PATHS:
        zj = load_fixture("zoeken", path)
        sj = load_fixture("searxng", path)
        notes.extend(compare_keys(path, zj, sj))
        required = REQUIRED_KEYS.get(path, set())
        issues.extend(check_required(f"{path} (zoeken)", zj, required))
        issues.extend(check_required(f"{path} (searxng)", sj, required))
    for note in notes:
        print(f"note: {note}")
    return report(issues)


def run_record(zoeken: str, searxng: str) -> int:
    for path in COMPARE_PATHS:
        _, zj = fetch_json(zoeken, path)
        _, sj = fetch_json(searxng, path)
        zpath = write_fixture("zoeken", path, zj)
        spath = write_fixture("searxng", path, sj)
        print(f"wrote {zpath.relative_to(ROOT)}")
        print(f"wrote {spath.relative_to(ROOT)}")
    return 0


def report(issues: list[str]) -> int:
    if not issues:
        print("compare_searxng: ok")
        return 0
    print("compare_searxng: differences / failures:")
    for issue in issues:
        print(f"  - {issue}")
    return 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("live", "fixtures", "record"))
    parser.add_argument("--zoeken", default="http://127.0.0.1:8888")
    parser.add_argument("--searxng", default="http://127.0.0.1:8080")
    args = parser.parse_args()

    if args.mode == "live":
        return run_live(args.zoeken, args.searxng)
    if args.mode == "record":
        return run_record(args.zoeken, args.searxng)
    return run_fixtures()


if __name__ == "__main__":
    sys.exit(main())
