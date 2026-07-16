# Data Asset Parity Matrix

Tracked assets: 13. Present: 8. Missing: 4.

| Asset | Status | Upstream candidates | Rust files | Notes |
| --- | --- | --- | --- | --- |
| bangs | present | external_bangs.json | external_bangs.json | bundled in zoeken/zoeken-data/data |
| currencies | present | currencies.json | currencies.json | bundled in zoeken/zoeken-data/data |
| units | present | wikidata_units.json | wikidata_units.json | bundled in zoeken/zoeken-data/data |
| engine traits | present | engine_traits.json | engine_traits.json | bundled in zoeken/zoeken-data/data |
| locales | present | locales.json | locales.json | bundled in zoeken/zoeken-data/data |
| user agents | present | useragents.json, gsa_useragents.txt | gsa_useragents.txt, useragents.json | bundled in zoeken/zoeken-data/data |
| tracker patterns | present | data/tracker_patterns.json, tracker_patterns.json | tracker_patterns.json | bundled in zoeken/zoeken-data/data |
| Ahmia blacklist | present | data/ahmia_blacklist.txt, ahmia_blacklist.txt, data/ahmia_blacklist.json, ahmia_blacklist.json | ahmia_blacklist.txt | bundled in zoeken/zoeken-data/data |
| DOI resolvers | missing | settings.yml |  | not bundled yet |
| engine descriptions | unknown-upstream | data/engines_languages.json, engines_languages.json |  | not bundled yet |
| autocomplete metadata | missing | autocomplete.py |  | not bundled yet |
| limiter config | missing | limiter.toml, botdetection |  | not loaded by DataBundle yet |
| info pages | missing | infopage, info |  | not loaded by DataBundle yet |
