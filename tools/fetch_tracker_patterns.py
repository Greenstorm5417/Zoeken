#!/usr/bin/env python3
"""Fetch ClearURLs rules and write zoeken/zoeken-data/data/tracker_patterns.json."""

from __future__ import annotations

import json
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "zoeken" / "zoeken-data" / "data" / "tracker_patterns.json"
URLS = [
    "https://rules1.clearurls.xyz/data.minify.json",
    "https://rules2.clearurls.xyz/data.minify.json",
    "https://raw.githubusercontent.com/ClearURLs/Rules/master/data.min.json",
]


def fetch() -> dict:
    last_error: Exception | None = None
    for url in URLS:
        try:
            with urllib.request.urlopen(url, timeout=30) as resp:
                return json.loads(resp.read().decode("utf-8"))
        except Exception as exc:  # noqa: BLE001 — try next mirror
            last_error = exc
    raise SystemExit(f"failed to fetch ClearURLs rules: {last_error}")


def convert(raw: dict) -> list[dict]:
    out: list[dict] = []
    for provider in raw.get("providers", {}).values():
        url = provider.get("urlPattern")
        rules = provider.get("rules") or []
        if not url or not rules:
            continue
        out.append(
            {
                "url": url.replace("\\\\", "\\"),
                "exceptions": [
                    exc.replace("\\\\", "\\") for exc in (provider.get("exceptions") or [])
                ],
                "rules": list(rules),
            }
        )
    return out


def main() -> None:
    rules = convert(fetch())
    OUT.write_text(json.dumps(rules, ensure_ascii=False, separators=(",", ":")), encoding="utf-8")
    print(f"wrote {len(rules)} providers -> {OUT}")


if __name__ == "__main__":
    main()
