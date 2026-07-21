#!/usr/bin/env python3
"""Generate SearXNG compatibility inventory documents.

The generator uses only the Python standard library so CI can run it without
extra setup. By default it reads a local `searxng/` checkout when present; pass
`--upstream /path/to/searxng` to use another checkout.
"""

from __future__ import annotations

import argparse
import ast
import json
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DOCS = ROOT / "docs" / "compatibility"
DEFAULT_UPSTREAM_URL = "https://github.com/searxng/searxng.git"

SYSTEMS = {
    "engines": ("searx/engines", "zoeken/zoeken-engines"),
    "plugins": ("searx/plugins", "zoeken/zoeken-plugins"),
    "search": ("searx/search", "zoeken/zoeken-search"),
    "results": ("searx/results.py", "zoeken/zoeken-results"),
    "settings": ("searx/settings*.py, searx/settings.yml", "zoeken/zoeken-settings"),
    "preferences": (
        "searx/preferences.py, searx/webapp.py",
        "zoeken/zoeken-prefs, zoeken/zoeken-server/src/preferences.rs",
    ),
    "routes": ("searx/webapp.py", "zoeken/zoeken-server/src/lib.rs"),
    "data": ("searx/data", "zoeken/zoeken-data/data"),
    "network": ("searx/network", "zoeken/zoeken-network"),
    "limiter": ("searx/limiter.py, searx/botdetection", "zoeken/zoeken-botdetect"),
    "templates": ("searx/templates", "zoeken/zoeken-server/src/static_assets.rs"),
    "static assets": ("searx/static", "zoeken/zoeken-server/src/static_assets.rs, logo/"),
    "autocomplete": ("searx/autocomplete.py", "zoeken/zoeken-autocomplete"),
    "translations": (
        "searx/translations.py, searx/languages.py",
        "zoeken/zoeken-data/data/locales.json",
    ),
}

# Rust engines with no distinct SearXNG module (intentional Zoeken additions).
ZOEKEN_ONLY_ENGINES = {
    "wikibooks": "Zoeken-only MediaWiki books engine; no distinct SearXNG module",
    "currency": "Zoeken-only currency conversion instant answer (ECB rates); "
    "SearXNG's equivalent is a Python answerer, not an engine module",
    "dictionary": "Zoeken-only dictionary definition instant answer (Wiktionary); "
    "no distinct SearXNG engine module",
    "translate": "Zoeken-only translation instant answer (MyMemory); no distinct "
    "SearXNG engine module",
    "weather": "Zoeken-only weather instant answer (wttr.in), gated to "
    "weather-shaped queries; no distinct SearXNG engine module",
}

UPSTREAM_DATA_ASSETS = {
    "bangs": ["external_bangs.json"],
    "currencies": ["currencies.json"],
    "units": ["wikidata_units.json"],
    "engine traits": ["engine_traits.json"],
    "locales": ["locales.json"],
    "user agents": ["useragents.json", "gsa_useragents.txt"],
    "tracker patterns": ["data/tracker_patterns.json", "tracker_patterns.json"],
    "Ahmia blacklist": [
        "data/ahmia_blacklist.txt",
        "ahmia_blacklist.txt",
        "data/ahmia_blacklist.json",
        "ahmia_blacklist.json",
    ],
    "DOI resolvers": ["settings.yml", "doi_resolvers.json"],
    "engine descriptions": ["data/engines_languages.json", "engines_languages.json"],
    "autocomplete metadata": ["autocomplete.py", "autocomplete_backends.json"],
    "limiter config": ["limiter.toml", "botdetection"],
    "info pages": ["infopage", "info", "info_pages.json"],
}

INTENTIONALLY_SKIPPED = {
    "apple_maps": "requires Apple Maps token/bootstrap not yet supported",
    "artic": "bespoke API engine not supported",
    "baidu": "bespoke regional engine not supported",
    "cloudflareai": "requires Cloudflare AI credentials and custom request flow",
    "command": "command engines require an explicit sandbox decision",
    "currency_convert": "online_currency processor specialization deferred",
    "deezer": "requires Deezer API credentials / bespoke media flow",
    "demo_offline": "SearXNG demo engine",
    "demo_online": "SearXNG demo engine",
    "duckduckgo_weather": "bespoke weather engine not supported",
    "dummy": "SearXNG dummy/test engine",
    "dummy-offline": "SearXNG dummy/test engine",
    "dummy_offline": "SearXNG dummy/test engine",
    "flickr": "requires Flickr API key / bespoke media flow",
    "flickr_noapi": "bespoke HTML scraper not supported",
    "freesound": "requires Freesound API key",
    "frinkiac": "bespoke media engine not supported",
    "google_cse": "requires Google CSE API key",
    "google_images": "bespoke Google images flow not supported",
    "json_engine": "generic framework helper, not a standalone engine",
    "kavunka_demo": "SearXNG demo engine",
    "mariadb_server": "database engines require explicit safe execution semantics",
    "material_icons": "bespoke icon engine not supported",
    "mediathekviewweb": "bespoke regional video engine not supported",
    "mongodb": "database engines require explicit safe execution semantics",
    "mysql_server": "database engines require explicit safe execution semantics",
    "neosearch": "bespoke engine not supported",
    "opensemantic": "requires OpenSemantic instance configuration",
    "pdbe": "bespoke science engine not supported",
    "podcast": "placeholder example in upstream",
    "presearch": "requires a live request-id preflight before search requests",
    "postgresql": "database engines require explicit safe execution semantics",
    "quark": "bespoke regional engine not supported",
    "scanr_structures": "bespoke science engine not supported",
    "searx_engine": "upstream engine base class, not a standalone engine",
    "seekninja": "bespoke engine not supported",
    "sogou_images": "bespoke regional images engine not supported",
    "spotify": "requires Spotify API credentials",
    "torznab": "requires Torznab indexer configuration",
    "valkey_server": "database engines require explicit safe execution semantics",
    "xpath": "generic framework helper, not a standalone engine",
    "youtube_api": "requires YouTube Data API key",
    "youtube_noapi": "bespoke YouTube scraper not supported",
}

PORTED_ALIASES = {
    "9gag": "ninegag",
    "swisscows_news": "swisscows",
}


@dataclass(frozen=True)
class UpstreamEngine:
    name: str
    module: str
    path: str
    categories: list[str]
    processor: str
    paging: bool
    safesearch: bool
    time_range: bool
    language_support: bool
    engine_traits: bool
    api_key: bool
    network: bool
    generic_candidate: bool


def rel(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def snake_to_words(name: str) -> str:
    return name.replace("_", " ")


def words_to_snake(name: str) -> str:
    return re.sub(r"[^a-z0-9]+", "_", name.lower()).strip("_")


def rust_modules() -> list[str]:
    engine_dir = ROOT / "zoeken" / "zoeken-engines" / "src" / "engines"
    return sorted(p.stem for p in engine_dir.glob("*.rs") if p.stem not in {"mod", "util"})


def rust_fixture_modules() -> set[str]:
    fixture_dir = ROOT / "zoeken" / "zoeken-engines" / "fixtures"
    if not fixture_dir.exists():
        return set()
    modules = {p.name for p in fixture_dir.iterdir() if p.is_dir()}
    # upstream module → fixture dir aliases (9gag fixtures, ninegag rust module)
    if "9gag" in modules:
        modules.add("ninegag")
    if "ninegag" in modules:
        modules.add("9gag")
    return modules


def generic_catalog_names() -> set[str]:
    catalog = ROOT / "zoeken" / "zoeken-engines" / "src" / "engines" / "generic_catalog.json"
    if not catalog.exists():
        return set()
    entries = json.loads(catalog.read_text(encoding="utf-8"))
    names = {str(entry["name"]) for entry in entries if "name" in entry}
    names.update(words_to_snake(name) for name in list(names))
    return names


def parse_literal_assignment(source: str, key: str) -> Any:
    match = re.search(rf"(?m)^{re.escape(key)}\s*=\s*(.+?)(?:\n\S|\Z)", source, re.DOTALL)
    if not match:
        return None
    raw = match.group(1).strip()
    if raw.endswith("\n"):
        raw = raw.rsplit("\n", 1)[0].strip()
    try:
        return ast.literal_eval(raw)
    except (SyntaxError, ValueError):
        return None


def parse_bool_assignment(source: str, key: str) -> bool:
    match = re.search(rf"(?m)^{re.escape(key)}\s*=\s*(True|False)", source)
    return bool(match and match.group(1) == "True")


def parse_processor(source: str) -> str:
    processor = parse_literal_assignment(source, "engine_type")
    if isinstance(processor, str):
        return processor
    if "def response(" in source and "def request(" in source:
        return "online"
    if "offline" in source[:1000].lower():
        return "offline"
    return "unknown"


def parse_upstream_engines(upstream: Path | None) -> list[UpstreamEngine]:
    if upstream is None:
        return []
    engine_dir = upstream / "searx" / "engines"
    if not engine_dir.exists():
        return []
    engines = []
    for path in sorted(engine_dir.glob("*.py")):
        if path.name.startswith("__"):
            continue
        source = path.read_text(encoding="utf-8", errors="replace")
        categories = parse_literal_assignment(source, "categories")
        if not isinstance(categories, list):
            categories = []
        categories = [str(c) for c in categories]
        about = parse_literal_assignment(source, "about")
        api_key = bool(isinstance(about, dict) and about.get("require_api_key"))
        module = path.stem
        generic_candidate = (
            bool(re.search(r"xpath|eval_xpath|cssselect|json\(\)|resp\.json", source))
            and module not in INTENTIONALLY_SKIPPED
        )
        engines.append(
            UpstreamEngine(
                name=snake_to_words(module),
                module=module,
                path=path.relative_to(upstream).as_posix(),
                categories=categories,
                processor=parse_processor(source),
                paging=parse_bool_assignment(source, "paging"),
                safesearch=parse_bool_assignment(source, "safesearch"),
                time_range=parse_bool_assignment(source, "time_range_support"),
                language_support=parse_bool_assignment(source, "language_support"),
                engine_traits="EngineTraits" in source or "traits" in source,
                api_key=api_key,
                network=bool(re.search(r"(?m)^network\s*=|using_tor_proxy|tor_proxy|proxies", source)),
                generic_candidate=generic_candidate,
            )
        )
    return engines


def parse_settings_generic_engines(upstream: Path | None) -> list[UpstreamEngine]:
    if upstream is None:
        return []
    settings = upstream / "searx" / "settings.yml"
    if not settings.exists():
        return []
    source = settings.read_text(encoding="utf-8", errors="replace")
    engines = []
    for block in re.split(r"(?m)^  - name:\s*", source)[1:]:
        first, _, rest = block.partition("\n")
        name = first.strip().strip("'\"")
        if not re.search(r"(?m)^\s+engine:\s+(xpath|json_engine)\s*$", rest):
            continue
        categories = []
        match = re.search(r"(?m)^\s+categories:\s*(.+)$", rest)
        if match:
            raw = match.group(1).strip()
            if raw.startswith("[") and raw.endswith("]"):
                categories = [item.strip().strip("'\"") for item in raw[1:-1].split(",") if item.strip()]
            elif raw and not raw.startswith("{"):
                categories = [raw.strip("'\"")]
        engines.append(
            UpstreamEngine(
                name=name,
                module=words_to_snake(name),
                path="searx/settings.yml",
                categories=categories,
                processor="online",
                paging=bool(re.search(r"(?m)^\s+paging:\s+true\s*$", rest)),
                safesearch=bool(re.search(r"(?m)^\s+safesearch:\s+true\s*$", rest)),
                time_range=bool(re.search(r"(?m)^\s+time_range_support:\s+true\s*$", rest)),
                language_support="{lang}" in rest or bool(re.search(r"(?m)^\s+language:\s+", rest)),
                engine_traits=False,
                api_key=False,
                network=False,
                generic_candidate=False,
            )
        )
    return engines


def classify_engine(
    upstream: UpstreamEngine,
    rust: set[str],
    generic_catalog: set[str],
) -> tuple[str, str | None]:
    if upstream.module in PORTED_ALIASES:
        module = PORTED_ALIASES[upstream.module]
        if module == "generic" or module in rust:
            return "ported", module
    if upstream.name in generic_catalog or upstream.module in generic_catalog:
        return "ported", "generic"
    if upstream.module in rust:
        return "ported", upstream.module
    candidates = {
        upstream.module.replace("_", ""),
        upstream.module.replace("_", "-"),
        words_to_snake(upstream.name),
    }
    for module in rust:
        if module in candidates or module.replace("_", "") == upstream.module.replace("_", ""):
            return "ported", module
    if upstream.module in INTENTIONALLY_SKIPPED:
        return "intentionally-skipped", None
    if upstream.generic_candidate:
        return "generic-candidate", None
    return "missing", None


def parse_upstream_routes(upstream: Path | None) -> list[dict[str, Any]]:
    if upstream is None:
        return []
    webapp = upstream / "searx" / "webapp.py"
    if not webapp.exists():
        return []
    routes = []
    source = webapp.read_text(encoding="utf-8", errors="replace")
    pattern = re.compile(r"@app\.route\('([^']+)'(?:,\s*methods=\[([^\]]+)\])?")
    for match in pattern.finditer(source):
        methods = ["GET"]
        if match.group(2):
            methods = [m.strip().strip("'\"") for m in match.group(2).split(",")]
        routes.append({"path": match.group(1), "methods": sorted(methods)})
    add_rule = re.compile(r"app\.add_url_rule\('([^']+)'.*?methods=\[([^\]]+)\]", re.DOTALL)
    for match in add_rule.finditer(source):
        routes.append(
            {
                "path": match.group(1),
                "methods": sorted(m.strip().strip("'\"") for m in match.group(2).split(",")),
            }
        )
    return sorted(routes, key=lambda r: r["path"])


def parse_rust_routes() -> list[dict[str, Any]]:
    lib = ROOT / "zoeken" / "zoeken-server" / "src" / "lib.rs"
    frontend = ROOT / "zoeken" / "zoeken-server" / "src" / "frontend.rs"
    source = lib.read_text(encoding="utf-8")
    routes = []
    # Match .route("path", ...) across newlines (balanced one-level parens).
    for match in re.finditer(
        r'\.route\(\s*"([^"]+)"\s*,\s*((?:[^()]|\([^()]*\))*)\)',
        source,
        re.DOTALL,
    ):
        call = match.group(2)
        methods = []
        if re.search(r"\bget\s*\(", call):
            methods.append("GET")
        if re.search(r"\bpost\s*\(", call):
            methods.append("POST")
        # Axum `{param}` → inventory `<param>` (SearXNG-style placeholders).
        path = re.sub(r"\{([^}]+)\}", r"<\1>", match.group(1))
        if path.startswith("/info/") and path.count("/") == 3:
            path = "/info/<locale>/<pagename>"
        if path.startswith("/logo/"):
            path = "/logo/<resolution>"
        routes.append({"path": path, "methods": sorted(set(methods)) or ["UNKNOWN"]})
    # `/client{token}.css` is served from the fallback (Axum path-segment limit).
    frontend_source = frontend.read_text(encoding="utf-8")
    if "pub async fn client_css_or_static" in frontend_source:
        routes.append({"path": "/client<token>.css", "methods": ["GET", "POST"]})
    return sorted(routes, key=lambda r: r["path"])


def count_path(base: Path | None, relative_spec: str) -> int:
    if base is None:
        return 0
    total = 0
    for spec in [s.strip() for s in relative_spec.split(",")]:
        if "*" in spec:
            total += len(list(base.glob(spec)))
            continue
        path = base / spec
        if path.is_dir():
            total += len([p for p in path.iterdir() if not p.name.startswith("__")])
        elif path.exists():
            total += 1
    return total


def count_rust(relative_spec: str) -> int:
    total = 0
    for spec in [s.strip() for s in relative_spec.split(",")]:
        path = ROOT / spec
        if path.is_dir():
            total += len([p for p in path.rglob("*") if p.is_file()])
        elif path.exists():
            total += 1
    return total


def build_inventory(upstream: Path | None) -> dict[str, Any]:
    systems = []
    for name, (upstream_spec, rust_spec) in SYSTEMS.items():
        systems.append(
            {
                "system": name,
                "upstream": upstream_spec,
                "rust": rust_spec,
                "upstream_count": count_path(upstream, upstream_spec),
                "rust_count": count_rust(rust_spec),
            }
        )
    return {"upstream": str(upstream) if upstream else None, "systems": systems}


def build_engines(upstream: Path | None) -> dict[str, Any]:
    rust = set(rust_modules())
    fixtures = rust_fixture_modules()
    generic_catalog = generic_catalog_names()
    rows = []
    upstream_engines = parse_upstream_engines(upstream) + parse_settings_generic_engines(upstream)
    for engine in upstream_engines:
        status, module = classify_engine(engine, rust, generic_catalog)
        fixture_status = "present" if module in fixtures or engine.module in fixtures else "missing"
        if module == "generic" and "generic" in fixtures:
            fixture_status = "present"
        if status != "ported":
            fixture_status = "not-applicable"
        rows.append(
            {
                "upstream_name": engine.name,
                "upstream_module": engine.module,
                "upstream_path": engine.path,
                "rust_module": module,
                "status": status,
                "categories": engine.categories,
                "processor_type": engine.processor,
                "paging": engine.paging,
                "safe_search": engine.safesearch,
                "time_range": engine.time_range,
                "language_support": engine.language_support,
                "engine_traits": engine.engine_traits,
                "api_key_required": engine.api_key,
                "network_required": engine.network,
                "fixture_status": fixture_status,
                "known_gaps": known_engine_gaps(status, engine),
            }
        )
    known = {r["rust_module"] for r in rows if r["rust_module"]}
    zoeken_only = sorted(name for name in rust if name in ZOEKEN_ONLY_ENGINES)
    orphaned = sorted(rust - known - set(ZOEKEN_ONLY_ENGINES))
    return {
        "summary": {
            "upstream_engines": len(rows),
            "rust_engines": len(rust),
            "ported": sum(1 for r in rows if r["status"] == "ported"),
            "generic_candidates": sum(1 for r in rows if r["status"] == "generic-candidate"),
            "missing": sum(1 for r in rows if r["status"] == "missing"),
            "intentionally_skipped": sum(1 for r in rows if r["status"] == "intentionally-skipped"),
            "zoeken_only_engines": len(zoeken_only),
            "orphaned_rust_engines": len(orphaned),
        },
        "engines": rows,
        "zoeken_only_engines": [
            {"name": name, "reason": ZOEKEN_ONLY_ENGINES[name]} for name in zoeken_only
        ],
        "orphaned_rust_engines": orphaned,
    }


def known_engine_gaps(status: str, engine: UpstreamEngine) -> list[str]:
    if status == "ported":
        gaps = []
        if engine.engine_traits:
            gaps.append("verify engine-traits parity")
        if engine.api_key:
            gaps.append("verify settings-driven API key plumbing")
        return gaps
    if status == "generic-candidate":
        return ["candidate for xpath/json generic engine framework"]
    if status == "intentionally-skipped":
        return [INTENTIONALLY_SKIPPED[engine.module]]
    return ["not ported"]


def build_routes(upstream: Path | None) -> dict[str, Any]:
    upstream_routes = parse_upstream_routes(upstream)
    rust_routes = parse_rust_routes()
    rust_by_path = {r["path"]: r for r in rust_routes}
    rows = []
    for route in upstream_routes:
        rust = rust_by_path.get(route["path"])
        rows.append(
            {
                "path": route["path"],
                "upstream_methods": route["methods"],
                "rust_methods": rust["methods"] if rust else [],
                "status": route_status(route, rust),
                "notes": route_notes(route["path"], rust),
            }
        )
    upstream_paths = {r["path"] for r in upstream_routes}
    for route in rust_routes:
        if route["path"] not in upstream_paths:
            rows.append(
                {
                    "path": route["path"],
                    "upstream_methods": [],
                    "rust_methods": route["methods"],
                    "status": "rust-only",
                    "notes": "Zoeken health/readiness or implementation-specific route",
                }
            )
    return {
        "summary": {
            "upstream_routes": len(upstream_routes),
            "rust_routes": len(rust_routes),
            "matching_paths": sum(1 for r in rows if r["status"] in {"ported", "partial"}),
            "missing": sum(1 for r in rows if r["status"] == "missing"),
        },
        "routes": sorted(rows, key=lambda r: r["path"]),
    }


def route_status(upstream_route: dict[str, Any], rust_route: dict[str, Any] | None) -> str:
    if rust_route is None:
        return "missing"
    if set(upstream_route["methods"]) == set(rust_route["methods"]):
        return "ported"
    return "partial"


def route_notes(path: str, rust_route: dict[str, Any] | None) -> str:
    if rust_route is None:
        return "route not present in Axum router"
    if path == "/search":
        return "schema/content negotiation parity still needs conformance fixtures"
    if path in {"/stats", "/stats/errors", "/metrics", "/config"}:
        return "implemented; schema parity needs golden tests"
    return "implemented path; status/header/body parity needs route tests"


def build_data_assets(upstream: Path | None) -> dict[str, Any]:
    local_files = {p.name for p in (ROOT / "zoeken" / "zoeken-data" / "data").glob("*") if p.is_file()}
    rows = []
    for asset, candidates in UPSTREAM_DATA_ASSETS.items():
        upstream_present = any(upstream_asset_exists(upstream, candidate) for candidate in candidates)
        local_present = any(Path(candidate).name in local_files for candidate in candidates)
        if local_present:
            status = "present"
        elif upstream_present:
            status = "missing"
        else:
            status = "unknown-upstream"
        rows.append(
            {
                "asset": asset,
                "upstream_candidates": candidates,
                "rust_files": sorted(f for f in local_files if f in {Path(c).name for c in candidates}),
                "status": status,
                "notes": data_asset_notes(asset, status),
            }
        )
    return {
        "summary": {
            "tracked_assets": len(rows),
            "present": sum(1 for r in rows if r["status"] == "present"),
            "missing": sum(1 for r in rows if r["status"] == "missing"),
        },
        "assets": rows,
    }


def upstream_asset_exists(upstream: Path | None, candidate: str) -> bool:
    if upstream is None:
        return False
    direct = upstream / "searx" / candidate
    data = upstream / "searx" / "data" / candidate
    return direct.exists() or data.exists()


def data_asset_notes(asset: str, status: str) -> str:
    if status == "present":
        return "bundled in zoeken/zoeken-data/data"
    if asset in {"tracker patterns", "Ahmia blacklist"}:
        return "needed by bundled plugins"
    return "not bundled yet"


def write_json(path: Path, data: Any) -> None:
    path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def table(headers: list[str], rows: list[list[Any]]) -> str:
    out = ["| " + " | ".join(headers) + " |", "| " + " | ".join(["---"] * len(headers)) + " |"]
    for row in rows:
        out.append("| " + " | ".join(md_cell(v) for v in row) + " |")
    return "\n".join(out) + "\n"


def md_cell(value: Any) -> str:
    if isinstance(value, list):
        value = ", ".join(str(v) for v in value)
    value = "" if value is None else str(value)
    return value.replace("|", "\\|").replace("\n", " ")


def write_inventory_md(data: dict[str, Any]) -> None:
    rows = [
        [
            s["system"],
            s["upstream"],
            s["rust"],
            s["upstream_count"] if s["upstream_count"] not in (None, "") else 0,
            s["rust_count"] if s["rust_count"] not in (None, "") else 0,
        ]
        for s in data["systems"]
    ]
    text = "# Compatibility Inventory\n\n"
    text += "Generated by `tools/compat_inventory.py`. Counts are filesystem inventory counts, not parity scores.\n\n"
    text += table(["System", "SearXNG source", "Rust target", "Upstream count", "Rust count"], rows)
    (DOCS / "inventory.md").write_text(text, encoding="utf-8")


def write_engines_md(data: dict[str, Any]) -> None:
    summary = data["summary"]
    text = "# Engine Compatibility Matrix\n\n"
    text += (
        f"Upstream engines: {summary['upstream_engines']}. Rust engines: {summary['rust_engines']}. "
        f"Ported: {summary['ported']}. Generic candidates: {summary['generic_candidates']}. "
        f"Missing: {summary['missing']}. Intentionally skipped: {summary['intentionally_skipped']}.\n\n"
    )
    rows = [
        [
            e["upstream_module"],
            e["status"],
            e["rust_module"] or "",
            e["categories"],
            e["processor_type"],
            yesno(e["paging"]),
            yesno(e["safe_search"]),
            yesno(e["time_range"]),
            yesno(e["language_support"]),
            yesno(e["api_key_required"]),
            yesno(e["network_required"]),
            e["fixture_status"],
            e["known_gaps"],
        ]
        for e in data["engines"]
    ]
    text += table(
        [
            "Upstream module",
            "Status",
            "Rust module",
            "Categories",
            "Processor",
            "Paging",
            "Safe",
            "Time",
            "Lang",
            "API key",
            "Network",
            "Fixtures",
            "Known gaps",
        ],
        rows,
    )
    if data.get("zoeken_only_engines"):
        text += "\n## Zoeken-Only Engines (No Upstream Module)\n\n"
        text += (
            "\n".join(
                f"- `{row['name']}` — {row['reason']}" for row in data["zoeken_only_engines"]
            )
            + "\n"
        )
    if data["orphaned_rust_engines"]:
        text += "\n## Rust Engines Not Matched To Upstream\n\n"
        text += "\n".join(f"- `{name}`" for name in data["orphaned_rust_engines"]) + "\n"
    (DOCS / "engines.md").write_text(text, encoding="utf-8")


def write_routes_md(data: dict[str, Any]) -> None:
    summary = data["summary"]
    text = "# Route And Schema Parity Matrix\n\n"
    text += (
        f"Upstream routes: {summary['upstream_routes']}. Rust routes: {summary['rust_routes']}. "
        f"Matching paths: {summary['matching_paths']}. Missing upstream paths: {summary['missing']}.\n\n"
    )
    rows = [
        [r["path"], r["status"], r["upstream_methods"], r["rust_methods"], r["notes"]]
        for r in data["routes"]
    ]
    text += table(["Path", "Status", "Upstream methods", "Rust methods", "Notes"], rows)
    (DOCS / "routes.md").write_text(text, encoding="utf-8")


def write_data_md(data: dict[str, Any]) -> None:
    summary = data["summary"]
    text = "# Data Asset Parity Matrix\n\n"
    text += (
        f"Tracked assets: {summary['tracked_assets']}. Present: {summary['present']}. "
        f"Missing: {summary['missing']}.\n\n"
    )
    rows = [
        [r["asset"], r["status"], r["upstream_candidates"], r["rust_files"], r["notes"]]
        for r in data["assets"]
    ]
    text += table(["Asset", "Status", "Upstream candidates", "Rust files", "Notes"], rows)
    (DOCS / "data-assets.md").write_text(text, encoding="utf-8")


def _count_status(rows: list[dict[str, Any]], key: str = "status") -> dict[str, int]:
    counts: dict[str, int] = {}
    for row in rows:
        status = row.get(key) or "unknown"
        counts[status] = counts.get(status, 0) + 1
    return counts


def write_scorecard(
    engines: dict[str, Any],
    routes: dict[str, Any],
    data_assets: dict[str, Any],
) -> None:
    """Generate docs/compatibility/scorecard.md from the matrices."""
    engine_counts = _count_status(engines["engines"])
    route_counts = _count_status(routes["routes"])
    data_counts = _count_status(data_assets["assets"])
    plugins_dir = ROOT / "zoeken" / "zoeken-plugins" / "plugins"
    plugin_files = sorted(p.stem for p in plugins_dir.glob("*.lua")) if plugins_dir.exists() else []
    client = ROOT / "zoeken-client"
    frontend = "SPA (zoeken-client → zoeken-server/assets)" if client.exists() else "missing"

    def fmt(counts: dict[str, int]) -> str:
        return ", ".join(f"{k}={v}" for k, v in sorted(counts.items()))

    text = f"""# Compatibility Scorecard

Generated by `tools/compat_inventory.py`. Regenerate with that script;
CI validates this file exists via `--check`.

## Summary

| Area | Status |
| --- | --- |
| Engines | {fmt(engine_counts)} (total {sum(engine_counts.values())}) |
| Routes | {fmt(route_counts)} (total {sum(route_counts.values())}) |
| Data assets | {fmt(data_counts)} (total {sum(data_counts.values())}) |
| Lua plugins on disk | {len(plugin_files)} (`{', '.join(plugin_files)}`) |
| Frontend | {frontend} |
| Near-term target | API + admin/config compatibility (`targets.md`) |

## Matrices

- Engines: [`engines.md`](engines.md) / [`engines.json`](engines.json)
- Routes: [`routes.md`](routes.md) / [`routes.json`](routes.json)
- Data: [`data-assets.md`](data-assets.md) / [`data-assets.json`](data-assets.json)
- Inventory: [`inventory.md`](inventory.md)
- Intentional gaps: [`intentional-differences.md`](intentional-differences.md)

## Validation tooling

- Side-by-side harness: `tools/compare_searxng.py` (`fixtures` / `live` / `record`)
- Engine fixtures: `zoeken-engines` conformance tests
- Security notes: [`../security/audit.md`](../security/audit.md)
"""
    (DOCS / "scorecard.md").write_text(text, encoding="utf-8")


def yesno(value: bool) -> str:
    return "yes" if value else "no"


def generate(upstream: Path | None) -> dict[str, Any]:
    DOCS.mkdir(parents=True, exist_ok=True)
    inventory = build_inventory(upstream)
    engines = build_engines(upstream)
    routes = build_routes(upstream)
    data_assets = build_data_assets(upstream)

    write_json(DOCS / "inventory.json", inventory)
    write_json(DOCS / "engines.json", engines)
    write_json(DOCS / "routes.json", routes)
    write_json(DOCS / "data-assets.json", data_assets)
    write_inventory_md(inventory)
    write_engines_md(engines)
    write_routes_md(routes)
    # targets.md is hand-maintained (frontend/data packaging notes); do not overwrite.
    write_data_md(data_assets)
    write_scorecard(engines, routes, data_assets)
    return {
        "inventory": inventory,
        "engines": engines,
        "routes": routes,
        "data_assets": data_assets,
    }


def validate_generated() -> None:
    engines = json.loads((DOCS / "engines.json").read_text(encoding="utf-8"))
    routes = json.loads((DOCS / "routes.json").read_text(encoding="utf-8"))
    data_assets = json.loads((DOCS / "data-assets.json").read_text(encoding="utf-8"))
    # Always refresh scorecard from matrices so --check stays self-healing.
    write_scorecard(engines, routes, data_assets)

    required = [
        "inventory.json",
        "inventory.md",
        "engines.json",
        "engines.md",
        "routes.json",
        "routes.md",
        "targets.md",
        "data-assets.json",
        "data-assets.md",
        "scorecard.md",
    ]
    missing = [name for name in required if not (DOCS / name).exists()]
    if missing:
        raise SystemExit(f"missing generated compatibility files: {', '.join(missing)}")

    rust = set(rust_modules())
    matrix_rust = {row["rust_module"] for row in engines["engines"] if row["rust_module"]}
    orphaned = set(engines.get("orphaned_rust_engines", []))
    zoeken_only = {
        row["name"] if isinstance(row, dict) else row
        for row in engines.get("zoeken_only_engines", [])
    }
    absent = rust - matrix_rust - orphaned - zoeken_only - set(ZOEKEN_ONLY_ENGINES)
    if absent:
        raise SystemExit(f"Rust engines absent from matrix: {', '.join(sorted(absent))}")
    missing_class = [row["upstream_module"] for row in engines["engines"] if not row["status"]]
    if missing_class:
        raise SystemExit(f"upstream engines without classification: {', '.join(missing_class)}")

    source_routes = {row["path"]: row["methods"] for row in parse_rust_routes()}
    documented_routes = {
        row["path"]: row["rust_methods"]
        for row in routes["routes"]
        if row["rust_methods"]
    }
    if source_routes != documented_routes:
        raise SystemExit(
            "checked-in route inventory does not match the Axum router; "
            "refresh it with an upstream checkout"
        )


def find_upstream(path: str | None, fetch: bool) -> tuple[Path | None, tempfile.TemporaryDirectory[str] | None]:
    if path:
        return Path(path).resolve(), None
    local = ROOT / "searxng"
    if (local / "searx" / "engines").exists():
        return local, None
    if fetch:
        tmp = tempfile.TemporaryDirectory(prefix="zoeken-searxng-")
        subprocess.run(
            ["git", "clone", "--depth", "1", DEFAULT_UPSTREAM_URL, tmp.name],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        return Path(tmp.name), tmp
    return None, None


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--upstream", help="Path to a SearXNG checkout")
    parser.add_argument("--fetch-upstream", action="store_true", help="Clone SearXNG to a temporary directory")
    parser.add_argument("--check", action="store_true", help="Validate generated compatibility files")
    args = parser.parse_args()

    if args.check:
        validate_generated()
        print("compatibility inventory files are present and internally consistent")
        return 0

    upstream, tmp = find_upstream(args.upstream, args.fetch_upstream)
    if upstream is None:
        raise SystemExit(
            "no SearXNG checkout found (expected ./searxng or pass --upstream / --fetch-upstream). "
            "Refusing to regenerate engines matrices from an empty upstream."
        )
    try:
        generated = generate(upstream)
    finally:
        if tmp is not None:
            tmp.cleanup()
    summary = generated["engines"]["summary"]
    print(
        "generated compatibility inventory: "
        f"{summary['upstream_engines']} upstream engines, {summary['rust_engines']} Rust engines"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
