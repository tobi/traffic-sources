# Third-party notices

`traffic-sources` is MIT licensed (see [LICENSE](LICENSE)). This file covers the *rule data*
shipped in [`rules/`](rules/) and the reference data under [`reference/ts/`](reference/ts/).

## Licensing policy for shipped data

**Only CC0, MIT, Apache-2.0, Unlicense, or vendor documentation may be copied into `rules/`.**

Rule sets under copyleft licences (GPL, AGPL, LGPL, EUPL) are **not vendored**. They may be cited
as prior art, and they can be loaded at runtime by the user, who then takes on whatever obligations
that licence imposes:

```rust
let attributor = Attributor::new().with_external_file("merged.json")?;
```

```console
$ traffic-sources --external merged.json < access.log
```

Every entry in `rules/rules.json` and `rules/devices.json` carries a `src` (documentation URL) and a
`verified` date, so the provenance of each individual fact is auditable. `rules/oss-referrers.json`
carries a single top-level `source` and `license`.

## Data sources vendored in `rules/`

| Source | Licence | What is used | Where |
|---|---|---|---|
| [matomo-org/searchengine-and-social-list](https://github.com/matomo-org/searchengine-and-social-list) | CC0-1.0 | Search-engine, social and AI-assistant host tables; per-engine search keyword params; Mastodon instance list | `rules/oss-referrers.json` (whole file), `rules/rules.json` → `referrers`, `mastodonInstances` |
| [ai-robots-txt/ai.robots.txt](https://github.com/ai-robots-txt/ai.robots.txt) | MIT | AI crawler / user-fetcher user-agent tokens, operator and function, used to split `ai_crawler` from `ai_user_fetch` | `rules/rules.json` → `botRules` |
| [ua-parser/uap-core](https://github.com/ua-parser/uap-core) | Apache-2.0 | Device, OS and in-app browser user-agent tokens, taken from the project's test corpus | `rules/devices.json`, `rules/rules.json` → `uaTokens` |
| [shalanah/inapp-spy](https://github.com/shalanah/inapp-spy) | MIT | In-app browser user-agent tokens (IABMV, Barcelona/Threads, and similar modern markers) | `rules/rules.json` → `uaTokens` |
| [monperrus/crawler-user-agents](https://github.com/monperrus/crawler-user-agents) | MIT | Search crawler, SEO tool, monitoring and link-preview bot tokens | `rules/rules.json` → `botRules` |
| [omrilotan/isbot](https://github.com/omrilotan/isbot) | Unlicense | Generic automation / HTTP-client tokens (`curl`, `python-requests`, headless browsers) | `rules/rules.json` → `botRules` |
| [umami-software/umami](https://github.com/umami-software/umami) | MIT | Cross-check for paid ad params and channel domain buckets | cross-reference only |
| [PostHog/posthog-js](https://github.com/PostHog/posthog-js) | MIT | Cross-check for the click-ID parameter list (`gad_source`, `oppref`, `_kx`, …) | cross-reference only |

Vendor documentation (Google, Microsoft, Meta, TikTok, Snapchat, LinkedIn, Pinterest, OpenAI,
Anthropic, Perplexity, Apple, Amazon, Huawei, Mozilla/MDN, Chromium, Android) is cited per rule in
the `src` field. Facts taken from public documentation are not copyrightable expression; the URL is
recorded so each rule can be re-checked.

## Cited but **not** vendored (copyleft)

None of the data from these projects is copied into this repository. They are listed because they
informed the design, and because users may legitimately want to load them at runtime.

| Project | Licence | Relationship |
|---|---|---|
| [snowplow-referer-parser/referer-parser](https://github.com/snowplow-referer-parser/referer-parser) | GPL-3.0 | De-facto standard referrer table. **Not vendored.** Convert and load via `--external` if you want it. |
| [plausible/analytics](https://github.com/plausible/analytics) | AGPL-3.0 | Reference implementation of the GA4 channel waterfall and the "`utm_source` ending in `ad`/`ads` is paid" heuristic. The behaviour was re-implemented from Google's published channel-group definition; no Plausible code or data file is copied. |
| [ClearURLs/Rules](https://github.com/ClearURLs/Rules) | LGPL-3.0 | Tracking-parameter catalogue. **Not vendored.** The four share-marker rules (`si`, `s`, `t`, `ref_src`) are bare parameter names observed directly on first-party share links and are annotated as such. |
| [Smile4ever/Neat-URL](https://github.com/Smile4ever/Neat-URL) | GPL-2.0 | Tracking-parameter catalogue. Not vendored. |
| [AdguardTeam/AdguardFilters](https://github.com/AdguardTeam/AdguardFilters) | GPL-3.0 | `$removeparam` tracking-parameter rules. Not vendored. |
| [matomo-org/device-detector](https://github.com/matomo-org/device-detector) | LGPL-3.0 | Most exhaustive user-agent regex set. Not vendored; `rules/devices.json` is built from uap-core (Apache-2.0) and vendor docs. |
| [arp242/goatcounter](https://github.com/arp242/goatcounter) | EUPL-1.2 | Pragmatic `android-app://` referrer handling. Idea only. |

The survey that produced this comparison is kept at
[`docs/oss-survey/`](docs/oss-survey/). Its `build.py` clones and merges several of the copyleft
sources above into a local `data/merged.json`; **that output is gitignored and must never be
vendored** into `rules/`. It exists so the merged table can be produced locally and loaded with
`--external`.

## Rust dependencies

`serde`, `serde_json`, `regex` and `url` are all MIT OR Apache-2.0. Run `cargo tree` or
[`cargo-deny`](https://github.com/EmbarkStudios/cargo-deny) for the full transitive list.
