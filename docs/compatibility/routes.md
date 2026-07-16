# Route And Schema Parity Matrix

Upstream routes: 22. Rust routes: 23. Matching paths: 22. Missing upstream paths: 0.

| Path | Status | Upstream methods | Rust methods | Notes |
| --- | --- | --- | --- | --- |
| / | ported | GET, POST | GET, POST | implemented path; status/header/body parity needs route tests |
| /about | ported | GET | GET | implemented path; status/header/body parity needs route tests |
| /autocompleter | ported | GET, POST | GET, POST | implemented path; status/header/body parity needs route tests |
| /clear_cookies | partial | GET | GET, POST | implemented path; status/header/body parity needs route tests |
| /client<token>.css | ported | GET, POST | GET, POST | implemented path; status/header/body parity needs route tests |
| /config | ported | GET | GET | implemented; schema parity needs golden tests |
| /engine_descriptions.json | ported | GET | GET | implemented path; status/header/body parity needs route tests |
| /favicon.ico | ported | GET | GET | implemented path; status/header/body parity needs route tests |
| /favicon_proxy | ported | GET | GET | implemented path; status/header/body parity needs route tests |
| /healthz | ported | GET | GET | implemented path; status/header/body parity needs route tests |
| /image_proxy | ported | GET | GET | implemented path; status/header/body parity needs route tests |
| /info/<locale>/<pagename> | ported | GET | GET | implemented path; status/header/body parity needs route tests |
| /logo/<resolution> | ported | GET | GET | implemented path; status/header/body parity needs route tests |
| /manifest.json | ported | GET | GET | implemented path; status/header/body parity needs route tests |
| /metrics | ported | GET | GET | implemented; schema parity needs golden tests |
| /opensearch.xml | ported | GET | GET | implemented path; status/header/body parity needs route tests |
| /preferences | ported | GET, POST | GET, POST | implemented path; status/header/body parity needs route tests |
| /readyz | rust-only |  | GET | Zoeken health/readiness or implementation-specific route |
| /robots.txt | ported | GET | GET | implemented path; status/header/body parity needs route tests |
| /rss.xsl | ported | GET, POST | GET, POST | implemented path; status/header/body parity needs route tests |
| /search | ported | GET, POST | GET, POST | schema/content negotiation parity still needs conformance fixtures |
| /stats | ported | GET | GET | implemented; schema parity needs golden tests |
| /stats/errors | ported | GET | GET | implemented; schema parity needs golden tests |
