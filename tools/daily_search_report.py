#!/usr/bin/env python3
"""Opt-in live latency and empty-rate check for the daily-search fixture.

Reads zoeken/zoeken-search/tests/fixtures/daily_queries.json. Without
ZOEK_LIVE_URL this prints the fixture size and exits 0. It does not score
relevance: live pages are not labeled here. Deterministic ranking and intent
numbers come from `cargo test -p zoeken-search --test daily_quality`.

    ZOEK_LIVE_URL=http://127.0.0.1:8888 uv run --no-project --python 3.13 tools/daily_search_report.py
"""

from __future__ import annotations

import json
import os
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "zoeken/zoeken-search/tests/fixtures/daily_queries.json"


def main() -> None:
    queries = json.loads(FIXTURE.read_text(encoding="utf-8"))
    print(f"fixture_queries={len(queries)}")
    base = os.environ.get("ZOEK_LIVE_URL", "").rstrip("/")
    if not base:
        print("live=skipped set ZOEK_LIVE_URL to measure latency and empty rate")
        return
    latencies = []
    empty = 0
    errors = 0
    for row in queries:
        url = f"{base}/search?format=json&q={urllib.parse.quote(row['q'])}"
        started = time.perf_counter()
        try:
            with urllib.request.urlopen(url, timeout=20) as response:
                payload = json.loads(response.read().decode("utf-8"))
        except (urllib.error.URLError, TimeoutError, json.JSONDecodeError) as exc:
            errors += 1
            print(f"error id={row['id']} {exc}")
            continue
        latencies.append((time.perf_counter() - started) * 1000)
        results = payload.get("results") or []
        if not results:
            empty += 1
    if not latencies:
        print(f"live_errors={errors} no successful responses")
        return
    latencies.sort()
    mid = latencies[len(latencies) // 2]
    print(
        f"live_ok={len(latencies)} live_errors={errors} "
        f"empty_rate={empty / len(queries):.3f} "
        f"latency_ms_p50={mid:.0f} latency_ms_max={latencies[-1]:.0f}"
    )
    print("relevance=not scored; fixture judgments are not live quality")


if __name__ == "__main__":
    main()
