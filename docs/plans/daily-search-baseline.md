# Daily-search baseline

Measured on this tree. Synthetic rows are not live provider quality.

## Fixture regression

Command: `cargo test --offline -p zoeken-search --test daily_quality -- --nocapture`

164 canned queries. Intent accuracy 1.000 against the fixture labels (the
labels follow the interpreter, so this is a regression lock). Dispatch
precision 1.000, recall 0.965. Weather rows count only when
`weather_request_location` would send a wttr.in request; other answer kinds
still follow the interpreter. Duplicate rate 0. Empty rate 0.860 is the share
of fixture queries whose produced result list is empty, including rows that
ship no hits. It is not a live empty rate. Production fusion MRR 1.000, RRF
MRR 1.000, navigational MRR 1.000 on the rows that have relevance judgments.
That saturation is why the fixture does not decide semantic rerank
(`rerank=false` in that printout). Fixture MRR is not live quality.

## Intent model

A hashed unigram/bigram logistic regression (1024 buckets) trains on sentences
that are not the held-out paraphrases, ambiguous lines, or traps. The unit
test `held_out_intent_is_disjoint_from_training` rejects any shared string.
Rules still run first. The model may label weather only when its score is at
least 0.75 and the rules said general. A score under 0.45 stays general. The
query string sent to ordinary engines is not rewritten.

Held-out, from the same command, 13 weather paraphrases and 12 traps. None of
those strings are training rows.

`rules_p=1.000 rules_r=0.000 p=1.000 r=0.846 fp=0 positives=13 traps=12 enable=true`

Rules recall on that set is 0 because the paraphrases do not contain
weather/forecast/wetter. The cascade recovers 11 of 13. No trap was labeled
weather. `model_enabled()` is true because of that comparison. A classified
weather query still produces a wttr.in request. On the fixture,
`cargo test -p zoeken-engines --lib fixture_weather` measured
`weather_dispatch tp=20 fp=0 fn=2 precision=1.000 recall=0.909`. The two
non-requests are the bare words "weather" and "forecast", which name no place.

## Semantic rerank

Same command, second line:

`representative baseline_mrr=0.738 rerank_mrr=0.738 ordinary=1.000->1.000 technical=1.000->1.000 nav=1.000->1.000 probe=0.067->1.000 candidates=24 latency_us=3233 bytes=864 enable=false`

The representative set is 24 titles and snippets per query (inside the 20–50
window). It mixes ordinary meanings of the embedding's synonym words, technical
queries, mixed topics whose justified hit is not first, and two navigational
hosts. Judgments are about those supplied titles, not the live web. The
embedding does not change that MRR (0.738 to 0.738) and does not hurt the
ordinary, technical, or navigational slices, so it does not earn a default.

The synonym probe is separate. It places the desired hit at position 15 and
moves MRR from 0.067 to 1.000, which shows the scorer can change rank 1. That
probe is the same vocabulary the embedding was trained on. It does not set
`semantic_rerank_enabled()`.

Default is off. `ZOEK_SEMANTIC_RERANK=1` forces it on and `0` forces it off.
When it runs, it scores at most the top 40 titles and snippets. Embedding-table
init and each candidate score sit inside a 5 ms budget; a slow or failed score
returns the aggregated order, then the two-per-domain cap is applied again.
No document bodies.

## Provider latency

`ZOEK_LIVE_URL=http://127.0.0.1:8888 uv run --no-project --python 3.13 tools/provider_latency_sample.py`

Six searches on 2026-11-11 against the running server. Wall time p50 856 ms,
max 3008 ms. Results per search 25–30. Five engines were unresponsive on every
search (captcha, access denied, or a bad response), and those failures were
fast. Sample averages: Bing 0.224 s, Brave 0.672 s, DuckDuckGo 0.145 s,
Wikidata 0.501 s, Wikipedia 0.023 s, weather 0.111 s. `request_timeout` stays
3.0. The cold search already hit that ceiling, and a lower cap would cut
Wikidata without fixing captcha or access-denied failures.

This sample is not a relevance judgment and not the 164-query fixture.

The same server, still the process that was already listening, then ran
`ZOEK_LIVE_URL=http://127.0.0.1:8888 uv run --no-project --python 3.13 tools/daily_search_report.py`
on 2026-11-11. All 164 fixture queries returned: `live_ok=164 live_errors=0
empty_rate=0.024 latency_ms_p50=976 latency_ms_max=3015`. The script does not
score relevance. The max still meets the 3.0 s ceiling, so `request_timeout`
stays 3.0.
