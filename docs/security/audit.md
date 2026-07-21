# Security Audit

Threat model: a public or semi-public metasearch instance. Attackers can send
arbitrary queries, forge headers, and try to turn zoeken into an open proxy.

## Controls that exist

| Area | Control | Tests |
| --- | --- | --- |
| Image proxy open-proxy / SSRF | HMAC on `url`, prefs/server enable gate, `validate_proxy_url` blocks loopback/private/link-local/CGNAT/metadata hosts + non-http(s); **redirects disabled**; body capped at 5 MiB | `zoeken-favicons` unit tests; `image_proxy.rs` route tests |
| Favicon proxy SSRF | HMAC on `authority`; `validate_proxy_authority` rejects private hosts including `host:port` / `[::1]:port`; resolver refuses blocked authorities; redirects disabled; 1 MiB body cap | `favicon_proxy.rs`, `proxy.rs` |
| Secret key | Required on non-loopback; rejects empty / short (&lt;16) / known placeholders | `secret_key_decision*` + `secret_key_is_weak*` |
| Secret leakage in logs | Request middleware redacts cookies, `Authorization`, secret headers, query strings | `redaction_prop.rs` |
| Spoofed client IP | `X-Forwarded-For` / `X-Real-IP` honored only when TCP peer ∈ `trusted_proxies`; otherwise peer IP. `deployment.trusted_proxies` is unioned into the limiter list at boot | `trusted_proxy_prop.rs`, `client_ip.rs`, `deployment_trusted_proxies_merge_into_non_empty_limiter_list` |
| Rate limiting | Botdetect limiter; 429 when exceeded; `public_instance` force-enables on non-loopback | `rate_limit_prop.rs`, `router_wiring_integration.rs`, `fail_open.rs` |
| Security headers / CSP | Default CSP + headers middleware; Google Fonts allowlisted for SPA | `security_headers_prop.rs` |
| Request limits | Body size + request timeout Tower layers from `deployment` settings | middleware unit tests |
| Cookies | Prefs cookie `HttpOnly` + `SameSite=Lax` (unsigned zlib+base64; HMAC still gates proxies) | prefs cookie tests |
| Command engines | Not implemented (hard fail-closed) | `intentional-differences.md` |
| Offline / DB engines | Settings-gated; no arbitrary shell | engine docs |
| Graceful shutdown | `/readyz` flips not-ready while draining; bounded grace period; `/healthz` and `/readyz` exempt from limiter | `serve_lifecycle_integration.rs` |

## Intentional non-goals / residual risk

- **DNS rebinding** after URL validation: host string is checked before fetch; a
  hostname that later resolves to a private IP is not re-checked on the
  connecting address. Mitigate by keeping image/favicon proxy off when untrusted,
  or front with an egress firewall.
- **`/metrics` and `/stats` auth**: set `general.open_metrics` (HTTP Basic
  password) on public instances; empty leaves `/stats` open and hides `/metrics`.
  Edge restriction remains optional defense in depth.
- **Per-request TLS verify disable**: fails closed (`intentional-differences.md`).
- **No CORS layer**: SPA is same-origin. Do not enable open CORS without review.
- **Theme cookie field**: still accepted for SearXNG cookie compatibility; SPA
  ignores it (OS `prefers-color-scheme` only).

## Operator checklist

1. Set a strong `server.secret_key` (≥16 random chars; required for non-loopback).
2. Leave `image_proxy` off unless needed; HMAC URLs come from result rendering.
3. Configure `trusted_proxies` only for real reverse proxies.
4. Enable the limiter for public instances (`public_instance` / limiter config).
5. Set `general.open_metrics` (Basic auth for `/metrics` and `/stats`); optionally also block them at the edge.
6. Keep `command` engines unsupported; do not add shell-out without a sandbox.
