# Daily-usability implementation audit

For the latest status after the agent's follow-up changes, see [the second audit](daily-usability-reaudit.md). Findings below record the earlier version and several have since been fixed.

Audited the working-tree changes against the 18-item implementation prompt on 2026-11-11. Base commit: `bdcbb07`. This is a review of implementation and verification evidence, not a claim about live provider quality. Application source was not changed during this audit.

**Verdict: the complete upgrade is not finished.** There are useful improvements, but essential experiments and browser validation are missing, required checks fail, and several user-facing regressions remain. Do not mark the original task complete.

## Verification performed

| Check | Result |
| --- | --- |
| `bun run test` | 140 tests passed across 28 files. Includes jsdom tests, not real-browser journeys. |
| `bun run check:types` | Passed. |
| `bun run lint` | Failed: 3 errors in ImageLightbox and ResultTemplates. |
| `bun run check` (the CI command) | Failed: 34 errors, including formatting and lint. |
| `bunx vite build --outDir /tmp/zoeken-audit-dist` | Passed. This checked Vite compilation, not the complete `build` script's data-sync step. |
| `cargo test -p zoeken-query -p zoeken-search --locked` | Passed. |
| `cargo test -p zoeken-server -p zoeken-engines --locked` | Failed to compile `json_roundtrip_prop`: ResultContainer initializer missing eight new fields. |
| Engine library tests | 227 passed, 50 ignored. |
| Server library tests | 175 passed outside the sandbox. The initial five socket-binding failures were sandbox restrictions, not application defects. |
| `cargo fmt --all --check` | Reported formatting differences in native serialization. |
| `git diff --check` | Reported trailing whitespace in generated native types. |
| Daily-quality fixture | Passed with 164 rows; only 23 contain ranking candidates and relevance judgments. |
| Browser/live-provider walkthrough | Not performed in this audit. No browser automation tool or installed browser was available in the inspected environment. No real-browser suite or walkthrough artifacts were found in the repository. |

Temporary command logs are under `/tmp/zoeken-audit-*.log`; they are not durable repository artifacts.

## Findings that prevent sign-off

### 1. Required backend tests do not compile

`zoeken/zoeken-server/tests/json_roundtrip_prop.rs:356` constructs ResultContainer without the newly added continuation/outcome/timing fields. The full server test command fails with E0063. Update all constructors and test the new response fields and defaults. Library tests alone do not cover this integration-test failure.

### 2. The advertised intent-model experiment is a hardcoded phrase heuristic

`zoeken/zoeken-query/src/interpretation.rs:225` checks whether text contains `sky in ` or `raining in `, excludes `news`, and assigns a constant confidence of 0.78. The frontend duplicates this logic in `queryInterpretation.ts`. There is no learned intent/entity model evaluation here.

More importantly, the weather engine still requires one of `weather`, `forecast`, or `wetter` in its independent request parser. A new label for `raining in seattle` does not cause that engine to fetch weather. The fixture's answer-trigger metrics check an intent-to-kind mapping, not actual answer production. The native interpretation also omits entities from its response, and the frontend reinterprets text rather than using the returned server interpretation.

Finish the ownership/dispatch contract, test actual answer requests and outcomes, and perform a genuine model-versus-rules experiment with held-out examples. Until then, describe this code as a heuristic, not evidence that NLP was evaluated.

### 3. The reranker experiment cannot establish semantic-search quality

`zoeken/zoeken-search/src/ranking.rs:148` computes literal token-frequency cosine and blends it with reciprocal baseline position. It does not implement or evaluate a semantic model.

The blend cannot even replace the first result: first place scores at least 0.85, whereas second place scores at most `0.85 / 2 + 0.15 = 0.575`. All lower places have still smaller maxima. The fixture reports MRR 1.000 for all three rankers, so there is no demonstrated quality gain or meaningful basis for rejecting semantic reranking.

Only 23 of 164 fixture rows contain ranked hits; their competing candidates are largely canned relevant-versus-"Unrelated" pairs. The fixture's empty rate is counted from rows labeled `empty`, not measured search behavior. No separate model latency/resource comparison was found.

Retain these as limited synthetic regressions, add realistic candidate sets and judgments, evaluate an actual semantic reranker, and record relevance/latency/resource tradeoffs honestly.

### 4. Answer arbitration suppresses a working date answer

`zoeken-client/src/lib/answers.ts:25` trusts the new inferred intent and drops valid answers when their kind does not match. Reproduced directly against the real helpers:

```text
query: days until 2027-01-01
interpret(query).intent: arithmetic
computeDateTimeAnswer(query): non-null date-math answer
presentAnswers([computedAnswer], [], query): []
```

The hyphens in an ISO date trigger arithmetic classification. Preserve valid deterministic answers when the predicted kind has no answer, and add false-positive regressions spanning every existing answerer. Confidence and provenance should affect prioritization rather than silently deleting valid output.

### 5. Pagination still conflates stopping with exhaustion

`zoeken-client/src/lib/pagination.ts:19` permanently stops at the first duplicate-only page, even when continuation is `more`. That can discard valid later pages. It also stops an empty `unknown` page after a transient provider failure. The backend never produces `More`; its continuation calculation uses current eligibility/result count rather than provider continuation signals and does not account for the next page's max-page boundary.

The numbered helper does not track repeat pages, so numbered and infinite pagination disagree. The search route hides all pagination when personal rules hide the current page's visible results, even if later pages could contain other domains.

Use bounded recovery/retry for unknown or repeated pages, respect explicit continuation, and keep a manual continuation path where appropriate. Test page 1 with A, page 2 repeating A, and page 3 with B; also test an empty failed page followed by recovery.

### 6. Operator chip removal is not a round trip

Directly reproduced against `zoeken-client/src/lib/queryInterpretation.ts:125`:

```text
filetype:.PDF rust  -> parsed filetype pdf -> remove chip -> unchanged query
site:"docs.rs" rust -> parsed site docs.rs -> remove chip -> unchanged query
```

The parser normalizes values, but removal searches for the normalized spelling rather than the source span. Preserve operator spans or original tokens and remove those. Cover repeated and quoted operators. Provider-specific translation remains absent; current post-filtering and limitation notes are a partial contract only.

### 7. Unicode handling remains inconsistent

The backend preserves more technical tokens, but `zoeken-search/src/text.rs` lowercases without canonical Unicode normalization and splits combining marks. The new frontend highlighter uses a different tokenizer.

Direct reproduction: `highlightPieces("café", "café")` returns an entirely unmarked result. `highlightPieces("site docs", "site:docs.rs rust")` highlights `site`, although the code attempts to exclude operator syntax. Align query interpretation/highlighting with deliberate Unicode and technical-token rules and test composed/decomposed text.

### 8. Deduplication still merges distinct locale pages

`zoeken/zoeken-search/src/aggregation.rs:629` still maps recognized locale root paths to `/`. Thus `/en`, `/fr`, and the root of the same host can collapse. This was explicitly called out in the original task. Non-default ports and general mobile hosts were improved, but locale preservation is not finished.

Source diversity also uses a homemade registrable-domain approximation; it groups independent tenants under domains such as `github.io`. Define the intended site boundary and use reliable suffix information or a conservative policy.

### 9. Browser acceptance coverage is missing

`zoeken-client/src/journeys/acceptance.test.tsx:1` explicitly selects jsdom. Most "journeys" call helpers directly; three render isolated components. They never run the real search route through typing, autocomplete, loading, filters, opening a result, browser back, pagination, or mobile layouts. No real-browser runner/configuration was added.

This matters beyond naming: the new More dropdown is absolutely positioned inside an `overflow-x-auto` category strip, which risks clipping its popover. Its secondary-category links also ignore `search_on_category_select=false`, unlike primary tabs. The image lightbox's new Previous/Next buttons sit outside the panel's key-event boundary; Enter/Space bubble to the backdrop's close handler. These need actual browser checks and targeted fixes.

## All 18 requirements

"Core implemented" means the inspected core behavior exists; it does not substitute for the missing browser acceptance evidence.

| # | Requirement | Status | Remaining work |
| --- | --- | --- | --- |
| 1 | Daily-search benchmark | Partial | 164 rows exist, but only 23 have ranking judgments. Add meaningful competing candidates, actual answer-trigger checks, measured empty/duplicate rates, and durable baseline reports. |
| 2 | Coherent query understanding | Partial; model experiment missing | Heuristics are duplicated and do not drive answer-provider dispatch. Return/use entities, align ownership, and evaluate a genuine model. |
| 3 | Tokenization and normalization | Partial | Technical tokens and Unicode letters improved; canonical normalization, combining marks, and frontend/backend consistency remain. |
| 4 | Ranking and semantic reranking | Incomplete experiment | RRF comparison exists. Token cosine is lexical, the top result is immovable, and the synthetic fixture cannot support a semantic-reranker decision. |
| 5 | Safe deduplication and diversity | Partial | Ports/mobile hosts improved and soft diversity added. Locale roots still collapse; domain grouping needs correction. |
| 6 | Engine/filter capabilities | Partial | General-category matching improved. Frontend eligibility excludes explicitly requested disabled engines and matches names only, whereas backend supports explicit disabled engines and shortcuts. Bang-derived selections, health, and page eligibility are not reflected. |
| 7 | First useful result latency | Partial | Search/autocomplete AbortSignals added. No provider critical-path study, measured budget tuning, or before/after latency evidence. Parallel execution still waits for the aggregate. Progressive delivery was conditional and is not itself mandatory without evidence. |
| 8 | Immediate local answers | Core implemented; regression remains | Rendering moved outside the network-result gate and category/page checks were added. Date answers can still be suppressed by arbitration; real slow/failing-route validation is missing. |
| 9 | Correction/autocomplete | Partial | Closest-candidate selection and cancellation added. Multiword technical queries are not protected if only some tokens are technical; names remain unhandled. Abort errors can clear cached suggestions because sequence changes happen after cache-hit/early-return branches. Cache keys include language without sending that language to the API, and omit provider context. |
| 10 | Search operators | Partial | Post-filtering, notes, and chips added. Chip round trips fail, provider translation is absent, and parser parity/unsupported syntax need coverage. |
| 11 | Pagination/counts | Partial | Retrieved-count label and continuation field added. Duplicate-only/failed pages prematurely stop; numbered and infinite behavior diverge; hidden results can block later pages. |
| 12 | Failures/empty states | Partial | Outcomes and collapsed partial-failure disclosure added. A successful HTTP response with outcome `failed` has no retry action; ordinary no-match pages still lack contextual recovery. Several new messages are hardcoded English. |
| 13 | Results-page redesign | Partial; visually unverified | Primary/secondary categories added. No real browser evidence. Check clipped More menu, secondary-tab behavior, answer/results column alignment, narrow widths, focus, and themes. |
| 14 | Result rows | Partial | Breadcrumbs, expanded snippets, visited tracking, and highlighting added. Highlighting fails on accented queries; provider names remain the disclosure summary; specialized templates do not receive the same enhancements. |
| 15 | Answer presentation | Partial; regression | Deduplication and tomorrow forecast fields added. Date answers are lost, provider interpretation remains fragmented, freshness/confidence selection and location ambiguity handling are unfinished. |
| 16 | Personal source controls | Core implemented | Local hide/prefer, undo, and preference management exist. Actions are only wired to ordinary ResultItem rows, not specialized/media rows. Verify persistence, blocked storage, and hidden-page continuation in a browser. |
| 17 | Maps/images | Partial | Blank/range coordinate checks and image aspect ratios/fallback/navigation added. Verify actual provider coordinate shapes and map/list interaction; fix lightbox keyboard activation and browser-test focus/navigation. |
| 18 | Browser acceptance suite | Missing | jsdom component/helper tests do not satisfy the requested real-browser journeys. Add deterministic provider mocks and actual browser CI coverage, with opt-in live checks separate. |

## Required completion gate for the implementing agent

1. Fix the compilation/check failures and demonstrated regressions before further feature work.
2. Finish the partial requirements above; update the original tracker honestly. Do not relabel heuristics or helper tests as completed ML experiments or browser journeys.
3. Run the real project CI commands, including all backend integration tests and native-type generation checks. Preserve existing behavior and privacy controls.
4. Run deterministic browser journeys on the complete application in mobile/desktop and light/dark modes. Record the actual outcomes and representative screenshots.
5. Produce a durable baseline/comparison report for quality and latency. Separate synthetic, live, and unmeasured claims.
6. Deliver an updated 18-item matrix tied to passing acceptance evidence. Any externally blocked check must remain explicitly unverified.
