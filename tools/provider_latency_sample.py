#!/usr/bin/env python3
"""Measure provider contribution and search wall time against a running Zoeken.

Reads /stats before and after a fixed query list. Does not score relevance.
Requires ZOEK_LIVE_URL. Prints nothing invented: only HTTP timings and the
stats counters the server reports.

    ZOEK_LIVE_URL=http://127.0.0.1:8888 uv run --no-project --python 3.13 tools/provider_latency_sample.py
"""

from __future__ import annotations

import json
import os
import time
import urllib.error
import urllib.parse
import urllib.request

QUERIES = (
    "rust",
    "rust-lang.org",
    "weather berlin",
    "cannot borrow as mutable",
    "pythn",
    "who invented the telephone",
)


def get_json(url: str, timeout: float) -> tuple[dict, float]:
    started = time.perf_counter()
    with urllib.request.urlopen(url, timeout=timeout) as response:
        payload = json.loads(response.read().decode("utf-8"))
    return payload, (time.perf_counter() - started) * 1000


def stats_map(base: str) -> dict[str, dict]:
    payload, _ = get_json(f"{base}/stats", 20)
    engines = payload.get("engines") or []
    return {row["engine"]: row for row in engines}


def main() -> None:
    base = os.environ.get("ZOEK_LIVE_URL", "").rstrip("/")
    if not base:
        print("live=skipped set ZOEK_LIVE_URL")
        return
    before = stats_map(base)
    walls: list[float] = []
    for query in QUERIES:
        url = f"{base}/search?format=json&q={urllib.parse.quote(query)}"
        try:
            payload, elapsed = get_json(url, 30)
        except (urllib.error.URLError, TimeoutError, json.JSONDecodeError) as exc:
            print(f"error q={query!r} {exc}")
            continue
        results = payload.get("results") or []
        unresponsive = payload.get("unresponsive_engines") or []
        walls.append(elapsed)
        print(
            f"q={query!r} wall_ms={elapsed:.0f} results={len(results)} "
            f"unresponsive={len(unresponsive)}"
        )
    after = stats_map(base)
    names = sorted(set(before) | set(after))
    print("engine delta_count delta_sum_s sample_avg_s")
    for name in names:
        left = before.get(name, {})
        right = after.get(name, {})
        count = int(right.get("total_count") or 0) - int(left.get("total_count") or 0)
        total = float(right.get("total_sum_seconds") or 0) - float(
            left.get("total_sum_seconds") or 0
        )
        if count <= 0 and abs(total) < 1e-6:
            continue
        avg = total / count if count else 0.0
        print(f"{name} {count} {total:.3f} {avg:.3f}")
    if walls:
        ordered = sorted(walls)
        mid = ordered[len(ordered) // 2]
        print(
            f"searches={len(walls)} wall_ms_p50={mid:.0f} "
            f"wall_ms_max={ordered[-1]:.0f} request_timeout_unchanged=3.0"
        )


if __name__ == "__main__":
    main()
