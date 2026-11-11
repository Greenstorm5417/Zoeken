# Daily usability re-audit

2026-11-11, second review of the working tree based on `bdcbb07`. This supersedes the current-status conclusions in `daily-usability-audit.md`, which remains a record of the first review.

**Verdict: substantially improved, but all 18 requirements are still not complete.** The earlier compilation, answer suppression, operator removal, and accented highlighting defects have been repaired. A real browser suite now exists and passes. Remaining failures and gaps are listed below. Application source was not edited during this review.

## Checks actually run

| Check | Result |
| --- | --- |
| `cargo test --workspace --locked` | Passed: 919 passed, 57 ignored, zero failed across reported suites. Ignored checks remain unverified. |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Failed in `zoeken-query/src/interpretation.rs:226`: needless borrow in `dot(&weights, &features)`. Further diagnostics may appear once this is fixed. |
| `cargo fmt --all --check` | Passed. |
| `bun run check` | Passed; existing Biome schema-version informational message remains. |
| `bun run check:types` | Passed. |
| `bun run test` | 145 passed across 28 files. |
| `bun run test:e2e` | 11 Playwright journeys passed; one opt-in live test skipped. |
| `bun run build` | Passed, including data sync. |
| Generated native types | Exported with `export-native-ts` to a temporary file and compared to checked-in types: identical. |
| Direct regression probes | Date answer retained; `filetype:.PDF` and quoted `site:` chips remove their operators; `café` highlights correctly. |
| Visual inspection | Viewed desktop light and mobile dark screenshots of the running frontend with mocked provider responses. |

Logs and screenshots are temporary `/tmp/zoeken-reaudit-*` artifacts. Dependency security audits and a live provider-quality benchmark were not run in this review.

## Remaining findings

### 1. Backend CI is still red

The required Clippy command fails at `zoeken/zoeken-query/src/interpretation.rs:226`. `weights` is already a reference; use `dot(weights, &features)` and rerun the full command. Passing the workspace tests does not satisfy the separate Clippy gate.

### 2. The new semantic model is too narrowly evaluated to enable as the default search policy

`zoeken/zoeken-search/src/ranking.rs:260` trains an eight-dimensional embedding table from eight handwritten synonym groups, plus selected negative pairs. It now performs learned vector scoring, so the previous claim that it only computes literal token overlap no longer applies.

However, `held_out_windows()` contains only three relevance queries, all built from the same vocabulary and synonym relations used in training, plus one navigation query. Each relevance case manually places the desired hit at position 15 and uses repeated filler documents. The baseline is that deliberately supplied order, not the actual production fusion result on representative search candidates.

This experiment demonstrates that the small embedding can recover its supplied synonym relations. It does not establish a general search-quality improvement. Yet `semantic_rerank_enabled()` automatically enables production reranking based on this experiment, with no instance configuration switch. Finish a representative evaluation with independent judgments and regressions for mixed topics, technical queries, and ordinary meanings of these words. Keep deployment opt-in or disabled until it earns default status.

The existing 164-query fixture is still a limited regression lock: only 23 rows have ranking candidates/judgments, all reported rankers saturate at MRR 1.000, and the empty rate comes from fixture bucket labels. The new baseline document correctly admits these limits; the requested quality evaluation remains incomplete.

### 3. The advertised 5 ms reranking fallback does not guard scoring

`ranking.rs:183` starts a budget and checks it while collecting the top 40 hits. The expensive `semantic_order()` call at line 197 has no deadline check, and the result is applied without checking elapsed time afterward. First-use experiment/model initialization happens before the timer starts.

The baseline document says scoring that exceeds 5 ms returns the original order, which the implementation does not enforce. Include scoring in the budget and preserve baseline ordering when it is exceeded. Define how first-use initialization is handled. Also verify that reranking after aggregation does not undo the source-diversity policy.

### 4. Intent evaluation contains training/test overlap

The comment above `WEATHER_TRAIN` says none of the fixture trap strings occur in training. Both `meeting in paris tomorrow` and `2 fast 2 furious soundtrack` occur verbatim in the training table and the daily fixture's traps.

The new logistic regression and weather-dispatch connection are real improvements. The claim of held-out trap performance still needs correction: establish disjoint training/evaluation data, use broader unseen phrasing and entity cases, and evaluate actual provider dispatch/answers. The fixture's trigger precision/recall still comes from intent labels rather than actual answer production.

### 5. Provider-latency work is explicitly unfinished

`docs/plans/daily-search-baseline.md` states that live provider latency, empty rate, and relevance were not measured and request timeouts were not retuned. Cancellation is implemented, but there is no provider critical-path comparison, demonstrated budget tuning, or measured improvement in time to useful results. This leaves item 7 partial. Progressive delivery remains conditional; it need not be added without evidence.

### 6. The browser suite does not cover the full acceptance contract

The 11 passing journeys are valuable. Missing or weak coverage includes:

- Opening a result, going back, and restoring loaded pages/scroll position.
- Numbered pagination and agreement with infinite scrolling.
- Actual filter changes, eligible-engine reductions, and recovery.
- Prefer-site persistence and management, beyond hide/undo.
- A real keyboard-only search/result journey: the current keyboard test presses Tab once and only asserts that the active element has a nonempty tag name.
- Superseded requests within the mounted SPA: the current stale-response test uses two `page.goto()` navigations, which replaces the document rather than exercising the same route's request lifecycle.
- Layout assertions beyond search-box width, and representative screenshots across themes and viewport sizes.

The current mock config exposes no enabled secondary-category engine, so the More-menu interaction is not exercised by those journeys.

### 7. Desktop answer/result alignment remains visibly inconsistent

In a 1440-pixel viewport, the calculator card begins around x=408, while the results begin at x=168 and the search input around x=215. The answer block uses a separately centered `max-w-[42rem]` container (`routes/search.tsx:380`), while the results use `max-w-6xl`. Align them within a shared page grid. The mobile screenshot is usable, but this desktop inconsistency keeps item 13 unfinished.

## Updated 18-item status

| # | Requirement | Current assessment |
| --- | --- | --- |
| 1 | Quality benchmark | Partial: fixture and honest limitations documented; representative judgments and actual answer-quality measurements remain. |
| 2 | Query understanding | Partial: trained weather classifier, entity output, and dispatch added; training/test overlap and limited coverage remain. |
| 3 | Tokenization | Earlier normalization/highlighting defects fixed and regressions pass; broader multilingual coverage remains limited. |
| 4 | Ranking/semantic evaluation | Partial: learned prototype exists, but default enablement is unsupported by the narrow experiment and the timing fallback is incomplete. |
| 5 | Deduplication/diversity | Earlier locale-root merge repaired; mobile/port behavior covered. Domain grouping still uses a heuristic with selected tenant exceptions. |
| 6 | Filter capabilities | Explicit disabled engines/shortcuts and paging handling improved; full browser filter/eligibility journey remains unverified. |
| 7 | First useful result latency | Partial: cancellation exists; provider measurement and evidence-based tuning are absent. |
| 8 | Immediate local answers | Implemented and slow/failed-network browser scenario passes; date-answer regression repaired. |
| 9 | Correction/autocomplete | Improved cancellation, cache handling, and correction protection; broad name/technical-query and provider-fallback acceptance is not demonstrated. |
| 10 | Operators | Earlier chip round-trip failures repaired; post-filtering/limitations exist. Provider-specific translation and full operator parity remain limited. |
| 11 | Pagination/counts | Duplicate-page recovery passes in-browser; numbered/back-navigation and provider continuation edge cases still lack acceptance evidence. |
| 12 | Failure recovery | Retry and filter-removal browser cases now pass. Complete localization and broader failure combinations still need review. |
| 13 | UI redesign | More-menu placement/behavior repaired; desktop answer/result alignment and full visual acceptance remain. |
| 14 | Result rows | Accented highlighting, snippet expansion, breadcrumbs and visited behavior improved. Specialized-template parity remains limited. |
| 15 | Answers | Date fallback and forecast behavior repaired; broad confidence/provenance, freshness, and location ambiguity requirements remain partial. |
| 16 | Source preferences | Hide/undo passes in-browser; prefer/persistence/management journey and specialized-result action parity remain. |
| 17 | Maps/images | Coordinate handling and lightbox keyboard activation improved and tested. Full map/list interaction and focus restoration require broader checks. |
| 18 | Browser acceptance | Real suite exists and passes; required journeys listed above are still missing or weak. |

## Completion gate

Fix Clippy; make model decisions configurable and grounded in independent evaluation; enforce the reranking time budget; finish the provider-latency investigation; close the remaining browser journeys; align the desktop layout; then update the 18-item completion matrix with actual evidence. Do not substitute a green subset of tests for completion of the product requirements.
