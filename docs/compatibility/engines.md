# Engine Compatibility Matrix

Upstream engines: 288. Rust engines: 54. Ported: 248. Generic candidates: 0. Missing: 0. Intentionally skipped: 40.

| Upstream module | Status | Rust module | Categories | Processor | Paging | Safe | Time | Lang | API key | Network | Fixtures | Known gaps |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1337x | ported | generic | files | online | yes | no | no | no | no | no | present |  |
| 360search | ported | generic | general | online | yes | no | yes | no | no | no | present |  |
| 360search_videos | ported | generic | videos | online | yes | no | no | no | no | no | present |  |
| 500px | ported | generic | images | online | yes | no | no | no | no | no | present |  |
| 9gag | ported | ninegag | social media | online | yes | no | no | no | no | no | present |  |
| acfun | ported | generic | videos | online | yes | no | no | no | no | no | present |  |
| adobe_stock | ported | generic |  | online | yes | no | no | no | no | no | present |  |
| ahmia | ported | generic | onions | online | yes | no | yes | no | no | no | present |  |
| alpinelinux | ported | generic | packages, it | online | yes | no | no | no | no | no | present |  |
| annas_archive | ported | generic | files, books | online | no | no | no | yes | no | no | present | verify engine-traits parity |
| ansa | ported | generic | news | online | yes | no | yes | no | no | no | present |  |
| apkmirror | ported | generic | files, apps | online | yes | no | no | no | no | no | present |  |
| apple_app_store | ported | apple_app_store | files, apps | online | no | yes | no | no | no | no | present |  |
| apple_maps | intentionally-skipped |  | map | online | no | no | no | no | no | no | not-applicable | requires Apple Maps token/bootstrap not yet supported |
| archlinux | ported | generic | it, software wikis | online | yes | no | no | yes | no | no | present | verify engine-traits parity |
| artic | intentionally-skipped |  | images | online | yes | no | no | no | no | no | not-applicable | bespoke API engine deferred past Phase 6 campaign |
| artstation | ported | generic | images | online | yes | no | no | no | no | no | present |  |
| arxiv | ported | arxiv | science, scientific publications | online | yes | no | no | no | no | no | present |  |
| astrophysics_data_system | ported | generic | science, scientific publications | online | yes | no | no | no | no | no | present |  |
| azure | ported | generic | it, cloud | online | no | no | no | no | no | no | present |  |
| baidu | intentionally-skipped |  |  | online | yes | no | yes | no | no | no | not-applicable | bespoke regional engine deferred past Phase 6 campaign |
| bandcamp | ported | bandcamp | music | online | yes | no | no | no | no | no | present |  |
| base | ported | generic | science | online | yes | no | no | no | no | no | present |  |
| bilibili | ported | generic | videos | online | yes | no | yes | no | no | no | present |  |
| bing | ported | bing | general, web | online | no | yes | no | no | no | no | present | verify engine-traits parity |
| bing_images | ported | generic | images, web | online | yes | yes | yes | no | no | no | present | verify engine-traits parity |
| bing_news | ported | generic | news | online | yes | no | yes | no | no | no | present | verify engine-traits parity |
| bing_videos | ported | generic | videos, web | online | yes | yes | yes | no | no | no | present | verify engine-traits parity |
| bitchute | ported | generic | videos | online | yes | no | no | no | no | no | present |  |
| boardreader | ported | generic | general, social media | online | yes | no | yes | yes | no | no | present | verify engine-traits parity |
| bpb | ported | generic | general | online | yes | no | no | no | no | no | present |  |
| brave | ported | brave |  | online | no | yes | no | no | no | no | present | verify engine-traits parity |
| braveapi | ported | generic | general, web | online | yes | yes | yes | no | no | no | present |  |
| bt4g | ported | generic | files | online | yes | no | yes | no | no | no | present |  |
| btdigg | ported | generic | files | online | yes | no | no | no | no | no | present |  |
| cachy_os | ported | generic | packages, it | online | yes | no | no | no | no | no | present |  |
| cara | ported | generic | images | online | yes | no | no | no | no | no | present |  |
| ccc_media | ported | generic | videos | online | yes | no | no | no | no | no | present |  |
| chatnoir | ported | generic | general | online | yes | no | no | no | no | no | present |  |
| chefkoch | ported | generic |  | online | yes | no | no | no | no | no | present |  |
| chinaso | ported | generic |  | online | yes | no | yes | no | no | no | present |  |
| cloudflareai | intentionally-skipped |  |  | online | no | no | no | no | no | no | not-applicable | requires Cloudflare AI credentials and custom request flow |
| command | intentionally-skipped |  |  | offline | yes | no | no | no | no | no | not-applicable | command engines require an explicit sandbox decision |
| core | ported | core | science, scientific publications | online | yes | no | no | no | no | no | present |  |
| crates | ported | crates | it, packages, cargo | online | yes | no | no | no | no | no | present |  |
| crossref | ported | crossref | science, scientific publications | online | yes | no | no | no | no | no | present |  |
| currency_convert | intentionally-skipped |  | currency, general | online_currency | no | no | no | no | no | no | not-applicable | online_currency processor specialization deferred |
| dailymotion | ported | dailymotion | videos | online | yes | yes | yes | yes | no | no | present | verify engine-traits parity |
| deepl | ported | generic | general, translate | online_dictionary | no | no | no | no | no | no | present |  |
| deezer | intentionally-skipped |  | music | online | yes | no | no | no | no | no | not-applicable | requires Deezer API credentials / bespoke media flow |
| demo_offline | intentionally-skipped |  | general | offline | no | no | no | no | no | no | not-applicable | SearXNG demo engine |
| demo_online | intentionally-skipped |  | general | online | yes | no | no | no | no | no | not-applicable | SearXNG demo engine |
| destatis | ported | generic |  | online | yes | no | no | no | no | no | present |  |
| deviantart | ported | generic | images | online | yes | no | no | no | no | no | present |  |
| devicons | ported | generic | images, icons | online | no | no | no | no | no | no | present |  |
| dictzone | ported | generic | general, translate | online_dictionary | no | no | no | no | no | no | present |  |
| digbt | ported | generic | videos, music, files | online | yes | no | no | no | no | no | present |  |
| discourse | ported | generic |  | online | yes | no | yes | no | no | no | present |  |
| docker_hub | ported | docker_hub | it, packages | online | yes | no | no | no | no | no | present |  |
| dogpile | ported | dogpile | general | online | yes | yes | no | no | no | no | present |  |
| doku | ported | generic | general | online | no | no | no | no | no | no | present |  |
| duckduckgo | ported | duckduckgo |  | online | no | no | no | yes | no | no | present | verify engine-traits parity |
| duckduckgo_definitions | ported | generic |  | online | no | no | no | no | no | no | present |  |
| duckduckgo_extra | ported | generic |  | online | yes | yes | no | yes | no | no | present | verify engine-traits parity |
| duckduckgo_weather | intentionally-skipped |  | weather | online | no | no | no | yes | no | no | not-applicable | bespoke weather engine deferred past Phase 6 campaign |
| duckduckgo_web | ported | generic | general | online | yes | no | no | no | no | no | present | verify engine-traits parity |
| duden | ported | generic | dictionaries | online | yes | no | no | no | no | no | present |  |
| dummy-offline | intentionally-skipped |  |  | offline | no | no | no | no | no | no | not-applicable | SearXNG dummy/test engine |
| dummy | intentionally-skipped |  |  | online | no | no | no | no | no | no | not-applicable | SearXNG dummy/test engine |
| ebay | ported | generic | shopping | online | yes | no | no | no | no | no | present |  |
| elasticsearch | ported | elasticsearch | general | online | yes | no | no | no | no | no | present |  |
| emojipedia | ported | generic |  | online | no | no | no | no | no | no | present |  |
| fdroid | ported | generic | files, apps | online | yes | no | no | no | no | no | present |  |
| findfiles | ported | generic | files | online | yes | no | no | no | no | no | present |  |
| findthatmeme | ported | generic | images | online | yes | no | no | no | no | no | present |  |
| fireball | ported | generic | general | online | no | yes | no | no | no | no | present |  |
| flaticon | ported | generic | images, icons | online | yes | no | no | no | no | no | present |  |
| flickr | intentionally-skipped |  | images | online | yes | no | no | no | no | no | not-applicable | requires Flickr API key / bespoke media flow |
| flickr_noapi | intentionally-skipped |  | images | online | yes | no | yes | no | no | no | not-applicable | bespoke HTML scraper deferred past Phase 6 campaign |
| freesound | intentionally-skipped |  |  | online | yes | no | no | no | no | no | not-applicable | requires Freesound API key |
| frinkiac | intentionally-skipped |  | images | online | no | no | no | no | no | no | not-applicable | bespoke media engine deferred past Phase 6 campaign |
| fyyd | ported | generic |  | online | yes | no | no | no | no | no | present |  |
| geizhals | ported | generic | shopping | online | yes | no | no | no | no | no | present |  |
| genius | ported | genius | music, lyrics | online | yes | no | no | no | no | no | present |  |
| giphy | ported | generic | images | online | yes | no | no | no | no | no | present |  |
| gitea | ported | generic | it, repos | online | yes | no | no | no | no | no | present |  |
| github | ported | github | it, repos | online | no | no | no | no | no | no | present |  |
| github_code | ported | github_code | code | online | yes | no | no | no | no | no | present |  |
| gitlab | ported | gitlab | it, repos | online | yes | no | no | no | no | no | present |  |
| gmx | ported | generic | general | online | yes | yes | yes | no | no | no | present |  |
| goodreads | ported | generic |  | online | yes | no | no | no | no | no | present |  |
| google | ported | google | general, web | online | yes | yes | yes | yes | no | no | present | verify engine-traits parity |
| google_cse | intentionally-skipped |  | general, web | online | yes | yes | yes | yes | no | no | not-applicable | requires Google CSE API key |
| google_images | intentionally-skipped |  | images, web | online | yes | yes | yes | yes | no | no | not-applicable | bespoke Google images flow deferred past Phase 6 campaign |
| google_news | ported | generic | news | online | no | yes | no | yes | no | no | present | verify engine-traits parity |
| google_play | ported | generic |  | online | no | no | no | no | no | no | present |  |
| google_scholar | ported | generic | science, scientific publications | online | yes | no | yes | yes | no | no | present | verify engine-traits parity |
| google_videos | ported | generic | videos, web | online | yes | yes | yes | yes | no | no | present | verify engine-traits parity |
| grokipedia | ported | generic | general | online | yes | no | no | no | no | no | present |  |
| hackernews | ported | hackernews | it | online | yes | no | yes | no | no | no | present |  |
| heexy | ported | generic | general | online | yes | yes | no | no | no | no | present |  |
| hex | ported | generic | it, packages | online | yes | no | no | no | no | no | present |  |
| huggingface | ported | generic | it, repos | online | no | no | no | no | no | no | present |  |
| il_post | ported | generic | news | online | yes | no | yes | no | no | no | present |  |
| imdb | ported | imdb | movies | online | no | no | no | no | no | no | present |  |
| imgur | ported | generic | images | online | yes | no | yes | no | no | no | present |  |
| ina | ported | generic | videos | online | yes | no | no | no | no | no | present |  |
| invidious | ported | invidious | videos, music | online | yes | no | yes | no | no | no | present |  |
| ipernity | ported | generic | images | online | yes | no | no | no | no | no | present |  |
| iqiyi | ported | generic | videos | online | yes | no | yes | no | no | no | present |  |
| iseek | ported | generic | general | online | yes | no | no | no | no | no | present |  |
| jisho | ported | generic | dictionaries | online | no | no | no | no | no | no | present |  |
| json_engine | intentionally-skipped |  |  | online | no | no | no | no | no | no | not-applicable | generic framework helper, not a standalone engine |
| kagi | ported | generic | general | online | yes | yes | yes | no | no | no | present |  |
| kickass | ported | generic | files | online | yes | no | no | no | no | no | present |  |
| lemmy | ported | lemmy | social media | online | yes | no | no | no | no | no | present |  |
| lib_rs | ported | generic | it, packages | online | no | no | no | no | no | no | present |  |
| libretranslate | ported | generic | general, translate | online_dictionary | no | no | no | no | no | no | present |  |
| lingva | ported | generic |  | online_dictionary | no | no | no | no | no | no | present |  |
| loc | ported | generic | images | online | yes | no | no | no | no | no | present |  |
| lucide | ported | generic | images, icons | online | no | no | no | no | no | no | present |  |
| luxxle | ported | generic |  | online | no | no | no | no | no | no | present |  |
| magnific | ported | generic | images | online | yes | no | no | no | no | no | present |  |
| marginalia | ported | marginalia | general, blogs | online | yes | yes | no | no | no | no | present |  |
| mariadb_server | intentionally-skipped |  |  | offline | yes | no | no | no | no | no | not-applicable | database engines require explicit safe execution semantics |
| mastodon | ported | mastodon | social media | online | no | no | no | no | no | no | present |  |
| material_icons | intentionally-skipped |  | images, icons | online | no | no | no | no | no | no | not-applicable | bespoke icon engine deferred past Phase 6 campaign |
| mediathekviewweb | intentionally-skipped |  | videos | online | yes | no | no | no | no | no | not-applicable | bespoke regional video engine deferred past Phase 6 campaign |
| mediawiki | ported | generic | general | online | yes | no | no | no | no | no | present |  |
| meilisearch | ported | meilisearch | general | online | yes | no | no | no | no | no | present |  |
| metacpan | ported | generic | it, packages | online | yes | no | no | no | no | no | present |  |
| microsoft_learn | ported | generic | it | online | yes | no | no | yes | no | no | present |  |
| mixcloud | ported | generic | music | online | yes | no | no | no | no | no | present |  |
| mojeek | ported | mojeek | general, web | online | yes | yes | yes | yes | no | no | present | verify engine-traits parity |
| mongodb | intentionally-skipped |  |  | offline | yes | no | no | no | no | no | not-applicable | database engines require explicit safe execution semantics |
| moviepilot | ported | generic | movies | online | yes | no | no | no | no | no | present |  |
| mozhi | ported | generic | general, translate | online_dictionary | no | no | no | no | no | no | present |  |
| mrs | ported | generic | social media | online | yes | no | no | no | no | no | present |  |
| mwmbl | ported | generic | general | online | no | no | no | no | no | no | present |  |
| mysql_server | intentionally-skipped |  |  | offline | yes | no | no | no | no | no | not-applicable | database engines require explicit safe execution semantics |
| naver | ported | generic |  | online | yes | no | yes | no | no | no | present |  |
| neocities | ported | generic | general, blogs | online | yes | no | no | no | no | no | present |  |
| neosearch | intentionally-skipped |  | general | online | no | no | no | no | no | no | not-applicable | bespoke engine deferred past Phase 6 campaign |
| niconico | ported | generic | videos | online | yes | no | yes | no | no | no | present |  |
| npm | ported | generic | it, packages | online | yes | no | no | no | no | no | present |  |
| nvd | ported | generic | it | online | yes | no | no | no | no | no | present |  |
| nyaa | ported | nyaa | files | online | yes | no | no | no | no | no | present |  |
| odysee | ported | generic | videos | online | yes | no | yes | yes | no | no | present | verify engine-traits parity |
| ollama | ported | generic | it, repos | online | no | no | no | no | no | no | present |  |
| open_meteo | ported | generic | weather | online | no | no | no | no | no | no | present |  |
| openalex | ported | generic | science, scientific publications | online | yes | no | no | no | no | no | present |  |
| openclipart | ported | generic | images | online | yes | no | no | no | no | no | present |  |
| openlibrary | ported | generic | general, books | online | yes | no | no | no | no | no | present |  |
| opensemantic | intentionally-skipped |  |  | online | no | no | no | no | no | no | not-applicable | requires OpenSemantic instance configuration |
| openstreetmap | ported | openstreetmap | map | online | no | no | no | yes | no | no | present |  |
| openverse | ported | openverse | images | online | yes | no | no | no | no | no | present |  |
| pdbe | intentionally-skipped |  | science | online | no | no | no | no | no | no | not-applicable | bespoke science engine deferred past Phase 6 campaign |
| peertube | ported | peertube | videos | online | yes | yes | yes | yes | no | no | present | verify engine-traits parity |
| pexels | ported | generic | images | online | yes | no | yes | no | no | no | present |  |
| photon | ported | photon | map | online | no | no | no | no | no | no | present |  |
| picjumbo | ported | generic | images | online | yes | no | no | no | no | no | present |  |
| pinterest | ported | generic | images | online | yes | no | no | no | no | no | present |  |
| piped | ported | piped |  | online | yes | no | no | no | no | no | present |  |
| piratebay | ported | piratebay | files | online | no | no | no | no | no | no | present |  |
| pixabay | ported | generic | images | online | yes | yes | yes | no | no | no | present |  |
| pixiv | ported | generic | images | online | yes | no | no | no | no | yes | present |  |
| pkg_go_dev | ported | generic | packages, it | online | no | no | no | no | no | no | present |  |
| podchaser | ported | generic |  | online | yes | no | no | no | no | no | present |  |
| postgresql | intentionally-skipped |  |  | offline | yes | no | no | no | no | no | not-applicable | database engines require explicit safe execution semantics |
| presearch | intentionally-skipped |  | general, web | online | yes | yes | yes | no | no | no | not-applicable | requires a live request-id preflight before search requests |
| privacywall | ported | generic |  | online | yes | yes | yes | no | no | no | present | verify engine-traits parity |
| public_domain_image_archive | ported | generic | images | online | yes | no | no | no | no | no | present |  |
| pubmed | ported | generic | science, scientific publications | online | no | no | no | no | no | no | present |  |
| pypi | ported | pypi | it, packages | online | no | no | no | no | no | no | present |  |
| quark | intentionally-skipped |  |  | online | yes | no | yes | no | no | no | not-applicable | bespoke regional engine deferred past Phase 6 campaign |
| qwant | ported | qwant |  | online | yes | yes | no | no | no | no | present | verify engine-traits parity |
| radio_browser | ported | generic | music, radio | online | yes | no | no | yes | no | no | present | verify engine-traits parity |
| recoll | ported | generic |  | online | yes | no | yes | no | no | no | present |  |
| reddit | ported | reddit | social media | online | no | no | no | no | no | no | present |  |
| repology | ported | generic |  | online | no | no | no | no | no | no | present |  |
| resulthunter | ported | generic |  | online | yes | yes | yes | no | no | no | present | verify engine-traits parity |
| reuters | ported | generic | news | online | yes | no | yes | no | no | no | present |  |
| rottentomatoes | ported | generic | movies | online | no | no | no | no | no | no | present |  |
| rumble | ported | generic | videos | online | yes | no | no | no | no | no | present |  |
| s1search | ported | generic | general | online | yes | no | no | no | no | no | present |  |
| scanr_structures | intentionally-skipped |  | science | online | yes | no | no | no | no | no | not-applicable | bespoke science engine deferred past Phase 6 campaign |
| searchzee | ported | generic |  | online | yes | no | no | no | no | no | present |  |
| searx_engine | intentionally-skipped |  |  | online | no | no | no | no | no | no | not-applicable | upstream engine base class, not a standalone engine |
| seekninja | intentionally-skipped |  | general | online | no | yes | no | no | no | no | not-applicable | bespoke engine deferred past Phase 6 campaign |
| selfhst | ported | generic | images, icons | online | no | no | no | no | no | no | present |  |
| semantic_scholar | ported | semantic_scholar | science, scientific publications | online | yes | no | no | no | no | no | present |  |
| senscritique | ported | senscritique | movies | online | yes | no | no | no | no | no | present |  |
| sepiasearch | ported | sepiasearch | videos | online | yes | yes | yes | yes | no | no | present | verify engine-traits parity |
| seznam | ported | generic | general, web | online | no | no | no | no | no | no | present |  |
| shopify_stock | ported | generic | images | online | yes | no | no | no | no | no | present |  |
| sogou | ported | generic | general | online | yes | no | yes | no | no | no | present |  |
| sogou_images | intentionally-skipped |  | images | online | yes | no | no | no | no | no | not-applicable | bespoke regional images engine deferred past Phase 6 campaign |
| sogou_videos | ported | generic | videos | online | yes | no | no | no | no | no | present |  |
| sogou_wechat | ported | generic | news | online | yes | no | no | no | no | no | present |  |
| solidtorrents | ported | solidtorrents | files | online | yes | no | no | no | no | no | present |  |
| solr | ported | generic |  | online | yes | no | no | no | no | no | present |  |
| soundcloud | ported | soundcloud | music | online | yes | no | no | no | no | no | present |  |
| sourcehut | ported | generic | it, repos | online | yes | no | no | no | no | no | present |  |
| spotify | intentionally-skipped |  | music | online | yes | no | no | no | no | no | not-applicable | requires Spotify API credentials |
| springer | ported | generic | science, scientific publications | online | yes | no | no | no | no | no | present |  |
| sqlite | ported | sqlite |  | offline | yes | no | no | no | no | no | missing |  |
| stackexchange | ported | stackexchange |  | online | yes | no | no | no | no | no | present |  |
| startpage | ported | startpage | general, web | online | yes | yes | yes | yes | no | no | present | verify engine-traits parity |
| startpagina | ported | generic | general | online | yes | yes | no | no | no | no | present |  |
| steam | ported | generic |  | online | no | no | no | no | no | no | present |  |
| stocksnap | ported | generic | images | online | yes | no | no | no | no | no | present |  |
| swisscows | ported | swisscows | general | online | yes | no | yes | no | no | no | present |  |
| swisscows_news | ported | swisscows | news | online | yes | no | yes | no | no | no | present |  |
| tagesschau | ported | generic | general, news | online | yes | no | no | no | no | no | present |  |
| tiger | ported | generic |  | online | yes | no | no | no | no | no | present |  |
| tineye | ported | generic | general | online_url_search | yes | no | no | no | no | no | present |  |
| tokyotoshokan | ported | generic | files | online | yes | no | no | no | no | no | present |  |
| tonline | ported | generic |  | online | yes | no | yes | no | no | no | present |  |
| tootfinder | ported | tootfinder | social media | online | no | no | no | no | no | no | present |  |
| torznab | intentionally-skipped |  |  | online | no | no | no | no | no | no | not-applicable | requires Torznab indexer configuration |
| translated | ported | generic | general, translate | online_dictionary | no | no | no | no | no | no | present |  |
| tubearchivist | ported | generic | videos | online | yes | no | no | no | no | no | present |  |
| tusksearch | ported | generic | general | online | yes | no | no | no | no | no | present |  |
| unsplash | ported | unsplash | images | online | yes | no | no | no | no | no | present |  |
| uxwing | ported | generic | images, icons | online | no | no | no | no | no | no | present |  |
| valkey_server | intentionally-skipped |  |  | offline | no | no | no | no | no | no | not-applicable | database engines require explicit safe execution semantics |
| vimeo | ported | vimeo | videos | online | yes | no | no | no | no | no | present |  |
| voidlinux | ported | generic | packages, it | online | no | no | no | no | no | no | present |  |
| vuhuv | ported | generic |  | online | yes | no | no | no | no | no | present |  |
| wallhaven | ported | generic | images | online | yes | no | no | no | no | no | present |  |
| wikicommons | ported | generic |  | online | yes | no | no | no | no | no | present |  |
| wikidata | ported | wikidata |  | online | no | no | no | yes | no | no | present | verify engine-traits parity |
| wikipedia | ported | wikipedia |  | online | no | no | no | yes | no | no | present | verify engine-traits parity |
| wolframalpha_api | ported | generic |  | online | no | no | no | no | no | no | present |  |
| wolframalpha_noapi | ported | generic |  | online | no | no | no | no | no | no | present |  |
| wordnik | ported | generic | dictionaries, define | online | no | no | no | no | no | no | present |  |
| wttr | ported | generic | weather | online | no | no | no | no | no | no | present |  |
| www1x | ported | generic | images | online | no | no | no | no | no | no | present |  |
| xpath | ported | generic |  | online | no | no | no | no | no | no | present |  |
| yacy | ported | yacy | general | online | yes | no | no | no | no | no | present |  |
| yahoo | ported | generic | general, web | online | yes | no | yes | no | no | no | present |  |
| yahoo_news | ported | generic | news | online | yes | no | no | no | no | no | present |  |
| yandex | ported | generic |  | online | yes | no | no | no | no | no | present |  |
| yandex_music | ported | generic | music | online | yes | no | no | no | no | no | present |  |
| yep | ported | generic |  | online | no | yes | no | yes | no | no | present | verify engine-traits parity |
| youtube_api | intentionally-skipped |  | videos, music | online | no | no | no | no | no | no | not-applicable | requires YouTube Data API key |
| youtube_noapi | intentionally-skipped |  | videos, music | online | yes | no | yes | no | no | no | not-applicable | bespoke YouTube scraper deferred past Phase 6 campaign |
| zlibrary | ported | generic |  | online | no | no | no | yes | no | no | present | verify engine-traits parity |
| abcnyheter | ported | generic | general | online | yes | no | no | yes | no | no | present |  |
| anaconda | ported | generic | it | online | yes | no | no | no | no | no | present |  |
| ayo | ported | generic | general | online | no | no | no | no | no | no | present |  |
| bitbucket | ported | generic | it, repos | online | yes | no | no | no | no | no | present |  |
| cl0q | ported | generic | general | online | yes | no | no | no | no | no | present |  |
| crowdview | ported | generic | general | online | no | no | no | no | no | no | present |  |
| encyclosearch | ported | generic | general | online | yes | no | no | no | no | no | present |  |
| erowid | ported | generic |  | online | yes | no | no | no | no | no | present |  |
| etymonline | ported | generic | dictionaries | online | yes | no | no | no | no | no | present |  |
| fastbot | ported | generic | general | online | no | no | no | no | no | no | present |  |
| fynd | ported | generic | general | online | yes | yes | no | no | no | no | present |  |
| gabanza | ported | generic | general | online | no | no | no | no | no | no | present |  |
| habrahabr | ported | generic | it | online | yes | no | no | no | no | no | present |  |
| hoogle | ported | generic | it, packages | online | no | no | no | no | no | no | present |  |
| kavunka_demo | intentionally-skipped |  | general | online | yes | no | no | no | no | no | not-applicable | SearXNG demo engine |
| kozmonavt | ported | generic |  | online | no | no | no | no | no | no | present |  |
| kukei | ported | generic | general, blogs | online | no | no | no | no | no | no | present |  |
| library_genesis | ported | generic | files | online | no | no | no | no | no | no | present |  |
| lobste_rs | ported | generic | it | online | no | no | no | no | no | no | present |  |
| mdn | ported | generic | it | online | yes | no | no | no | no | no | present |  |
| mankier | ported | generic | it | online | no | no | no | no | no | no | present |  |
| openairedatasets | ported | generic | science | online | yes | no | no | no | no | no | present |  |
| openairepublications | ported | generic | science | online | yes | no | no | no | no | no | present |  |
| openrepos | ported | generic | files | online | yes | no | no | no | no | no | present |  |
| packagist | ported | generic | it, packages | online | yes | no | no | no | no | no | present |  |
| pub_dev | ported | generic | packages, it | online | yes | no | no | no | no | no | present |  |
| rawweb | ported | generic | general, blogs | online | yes | no | no | no | no | no | present |  |
| searchmysite | ported | generic | general, blogs | online | yes | no | no | no | no | no | present |  |
| tmdb | ported | generic | movies | online | yes | no | no | no | no | no | present |  |
| torch | ported | generic | onions | online | yes | no | no | no | no | no | present |  |
| unobtanium | ported | generic | general, blogs | online | yes | no | no | no | no | no | present |  |
| wiby | ported | generic | general, blogs | online | yes | no | no | no | no | no | present |  |
| rubygems | ported | generic | it, packages | online | yes | no | no | no | no | no | present |  |
| reloado | ported | generic | general | online | yes | no | no | yes | no | no | present |  |
| searchch | ported | generic |  | online | yes | no | no | yes | no | no | present |  |
| woxikon_de_synonyme | ported | generic | dictionaries | online | no | no | no | yes | no | no | present |  |
| wikimini | ported | generic | general | online | no | no | no | yes | no | no | present |  |
| xonaly | ported | generic | general | online | no | no | no | no | no | no | present |  |
| zapmeta | ported | generic | general | online | yes | no | no | no | no | no | present |  |
| sina | ported | generic | news | online | yes | no | yes | yes | no | no | present |  |

## Zoeken-Only Engines (No Upstream Module)

- `wikibooks` — Zoeken-only MediaWiki books engine; no distinct SearXNG module
