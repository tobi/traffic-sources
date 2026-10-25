# traffic-sources

[![CI](https://github.com/tobi/traffic-sources/actions/workflows/ci.yml/badge.svg)](https://github.com/tobi/traffic-sources/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/traffic-sources.svg)](https://crates.io/crates/traffic-sources)
[![docs.rs](https://docs.rs/traffic-sources/badge.svg)](https://docs.rs/traffic-sources)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Work out where a web request actually came from, using only what the request itself carries: the
URL, the `Referer`, the `User-Agent`, and a handful of other headers. No cookies, no JavaScript, no
network calls — so it works on raw access logs after the fact, as well as inline.

Three things set it apart from a typical referrer parser:

- **It returns every plausible answer, ranked.** A request with `twclid` *and* an X in-app-browser
  user agent is probably a paid X click, but it might be an organic one. You get both hypotheses
  with a confidence in `0.0..=1.0` rather than one guess.
- **Every answer shows its work.** Each candidate points at the evidence that produced it, and each
  piece of evidence carries the documentation URL the rule came from plus the date that URL was last
  checked.
- **It classifies the client, not just the channel.** OS, OS version, form factor (phone / tablet /
  desktop / TV / console / XR headset / car / wearable), browser, and the *surface* — standalone
  browser vs. in-app WebView vs. Android Custom Tab vs. AMP cache vs. webmail — plus a nine-class
  bot taxonomy that separates AI crawlers you want to exclude from user-triggered AI fetches that
  represent a real person.

## Quick start

### Library

```toml
[dependencies]
traffic-sources = "0.3"
```

```rust
use traffic_sources::{Attributor, Request};

// Compiles every rule set. `Attributor` is Send + Sync — build it once, share it.
let attributor = Attributor::new();

let req = Request {
    url: "https://shop.example/p?twclid=abc".into(),
    referer: Some("https://t.co/".into()),
    user_agent: Some(
        "Mozilla/5.0 (iPhone; CPU iPhone OS 18_6_2 like Mac OS X) AppleWebKit/605.1.15 \
         (KHTML, like Gecko) Mobile/22G100 Twitter for iPhone/12.31.2".into(),
    ),
    self_hosts: vec!["shop.example".into()],
    ..Default::default()
};

let a = attributor.attribute(&req);

assert_eq!(a.top().channel.as_str(), "paid_social");
assert_eq!(a.top().source, "x");
assert_eq!(a.client.surface.as_str(), "webview_iab");
```

That request produces two candidates, because both readings are defensible:

```text
paid_social      x            0.997
organic_social   x            0.515
```

and three pieces of evidence, each with its source:

```text
click_id:twclid    w=0.93  https://leadtrackr.io/learn/advertising-click-ids
ua_token:x         w=0.85  https://github.com/ua-parser/uap-core/blob/master/regexes.yaml
referer:t.co       w=0.75  https://github.com/matomo-org/searchengine-and-social-list (CC0)
```

Run the whole thing with `cargo run --example basic` ([`examples/basic.rs`](examples/basic.rs)).

### CLI

```console
$ cargo install traffic-sources
```

Reads NDJSON `Request` objects **or** Apache/nginx combined-log lines on stdin, writes one JSON
object per line on stdout:

```console
$ traffic-sources [--host shop.example] [--external merged.json] [--top] < access.log > out.ndjson
```

| Flag | Meaning |
|---|---|
| `--host HOST` | Your own hostname, used to resolve relative log paths and to detect internal traffic. Default `localhost`. |
| `--external FILE` | Additionally load a referrer table at runtime. Nothing copyleft ships in this crate (see [licensing](#licensing-and-data-sources)); this is how you opt into such a list yourself. |
| `--top` | Emit only the winning candidate plus `client` and `bot`, instead of the full ranked result. |

```console
$ echo '203.0.113.9 - - [24/Oct/2026:10:00:00 +0000] "GET /p HTTP/1.1" 200 1024 \
  "https://l.instagram.com/" "Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) \
  AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148 Instagram 395.1.4.56.91"' \
  | traffic-sources --host shop.example --top
{"bot":null,
 "client":{"app":"instagram","form_factor":"phone","os":"ios","os_version":"18.6",
           "surface":"webview_iab","browser":null,"device":"Apple","confidence":0.85,…},
 "top":{"channel":"organic_social","source":"instagram","medium":"social","paid":"organic",
        "interaction":"click","confidence":0.963,"share":1.0,"evidence":[0,1]},
 "url":"https://shop.example/p"}
```

## API

Full reference: **[docs/API.md](docs/API.md)**. The shape of it:

```rust
impl Attributor {
    pub fn new() -> Self;                                     // embedded rules
    pub fn from_json(rules: &str, devices: &str, oss: &str) -> Result<Self, RuleError>;
    pub fn with_external_referrers(self, json: &str) -> Result<Self, RuleError>;
    pub fn with_external_file(self, path: &str) -> Result<Self, RuleError>;

    pub fn attribute(&self, req: &Request) -> Attribution;    // all candidates, ranked
    pub fn classify_client(&self, req: &Request) -> Client;   // OS / form factor / surface / app
    pub fn classify_bot(&self, req: &Request) -> Option<Bot>;
    pub fn ga4_channel(&self, source: &str, medium: &str, campaign: &str, ref_host: &str)
        -> (Channel, bool);
}

pub fn parse_combined_log(line: &str, default_host: &str) -> Option<Request>;
```

`attribute` returns an `Attribution`:

| Field | What it holds |
|---|---|
| `candidates: Vec<Candidate>` | Every hypothesis, highest confidence first. Never empty — the floor is `direct`. |
| `client: Client` | OS, OS version (+ whether UA reduction froze it), form factor, device, browser, surface, host app. |
| `bot: Option<Bot>` | Class, token, operator, and `exclude` — whether to drop the hit from human analytics. |
| `is_prefetch` | `Sec-Purpose: prefetch` and friends. |
| `new_touch` | Whether this starts a session. False for internal, bot and prefetch traffic. |
| `evidence: Vec<Evidence>` | Every signal found, with weight, `rule_src` and `verified`. Candidates index into this. |

Everything serialises to snake_case JSON via serde, so the CLI output and the Rust types are the
same thing.

## Taxonomy

**Channel** — the GA4 default channel group, plus four of our own:

> `direct` · `organic_search` · `paid_search` · `organic_social` · `paid_social` ·
> `organic_video` · `paid_video` · `organic_shopping` · `paid_shopping` · `display` ·
> `cross_network` · `paid_other` · `ai_assistant` · `paid_ai` · `email` · `affiliate` ·
> `referral` · `audio` · `sms` · `mobile_push` · **`internal`** · **`dark_social`** · **`bot`** ·
> **`unknown`**

`ai_assistant` and `paid_ai` exist because ChatGPT, Perplexity, Claude and friends are now a real
acquisition channel that GA4 folds into Referral; `oppref` (OpenAI's ads click ID) maps to `paid_ai`.
`dark_social` is the honest label for traffic that is clearly an external click but carries no
referrer — a messaging-app WebView, or a `Sec-Fetch-Site: cross-site` navigation with nothing else
to go on.

**Surface** — `standalone_browser`, `webview_iab`, `custom_tab`, `pwa`, `twa`, `amp_cache`,
`email_client`, `webmail`, `bot`, and so on. Four values (`safari_view_controller`, `app_clip`,
`instant_app`, `native_app`) exist in the enum but are **never emitted**, because the headers do not
distinguish them — SFSafariViewController sends Safari's own user agent. They are there so you can
set them from a signal we don't have.

**Form factor** — `phone`, `tablet`, `desktop`, `tv`, `console`, `xr_headset`, `wearable`, `car`,
`e_reader`, `smart_display`, `embedded`, `unknown`.

**Bot class** — `search_crawler`, `ai_crawler`, `ai_search_crawler`, `ai_user_fetch`, `ad_verifier`,
`link_preview`, `seo_tool`, `monitoring`, `prefetch`, `automation`. The split that matters:
`ai_crawler` (GPTBot, ClaudeBot, CCBot) is training traffic you should exclude, while
`ai_user_fetch` (ChatGPT-User, Perplexity-User, Claude-User) is a human who asked a question and is
being fetched on their behalf.

Also `Os` (29 values, including TV, console and XR platforms), `Paid`, `Interaction`, and
`SignalKind`. See [docs/API.md](docs/API.md) for the complete lists.

## How confidence works — and what it is not

Each signal becomes an `Evidence` with a hand-set likelihood weight `w` and the channel and/or
source it points at:

| Signal | Weight |
|---|---|
| Full UTM triple | 0.95 |
| Click ID (`gclid`, `msclkid`, `ttclid`, …) | 0.93 |
| In-app UA token, `X-Requested-With` | 0.85 |
| Partial UTM, `android-app://` referrer | 0.80 |
| Curated referrer host | 0.75 |
| Weak click ID (`fbclid`, `igshid`, `srsltid`), share marker | 0.70 |
| Matomo OSS host table | 0.65 |
| Externally loaded list | 0.60 |
| Direct | 0.50 |
| Dark social from `Sec-Fetch` / WebView only | 0.45 / 0.35 |

A candidate `(channel, source)` collects every compatible evidence item and combines them with a
noisy-OR:

```text
P = 1 − Π(1 − wᵢ · fᵢ)
```

where `f = 1` for the same source, `0.8` for the same source *family* (Meta covers
facebook/instagram/messenger/threads). Then, for each explicit UTM or click ID that names the same
source but a *different* channel, the score is discounted: `P ← P · (1 − 0.5·w)`. That is what pulls
organic down to 0.515 in the example above when a `twclid` is present.

Source-only evidence — an in-app UA token, `X-Requested-With`, an organic `google.com` referrer —
supports *every* channel for that source rather than picking one. That is deliberate: seeing
`google.com` in the `Referer` must not argue against a `gclid` on the same request.

**What these numbers are not.** The weights are hand-set judgements, not learned from labelled data,
and nobody has calibrated them against ground truth. Treat `confidence` as a *ranking* and a rough
ordinal belief per hypothesis — "0.99 is much safer than 0.6" — not as a probability you can
multiply into a revenue model. The candidates are independent beliefs and do not sum to 1; `share`
gives the normalised view if you need one. If you have labelled conversion data, refit the weights;
they are all in one place, [`weights`](src/lib.rs), and `from_json` lets you swap the rules wholesale.

Known limits are tracked honestly in [docs/CRITIQUE.md](docs/CRITIQUE.md), including the rules whose
`verified` field is `null` because they have not been re-checked against a primary source.

## Licensing and data sources

The code is MIT. The **shipped rule data is restricted to CC0, MIT, Apache-2.0, Unlicense and vendor
documentation** so that this crate can be vendored into any project without copyleft obligations:

| Source | Licence | Used for |
|---|---|---|
| [matomo-org/searchengine-and-social-list](https://github.com/matomo-org/searchengine-and-social-list) | CC0-1.0 | Search / social / AI-assistant host tables, search keyword params, Mastodon instances |
| [ai-robots-txt/ai.robots.txt](https://github.com/ai-robots-txt/ai.robots.txt) | MIT | AI crawler vs. user-fetcher split, operator names |
| [ua-parser/uap-core](https://github.com/ua-parser/uap-core) | Apache-2.0 | Device / OS / in-app browser UA tokens |
| [shalanah/inapp-spy](https://github.com/shalanah/inapp-spy) | MIT | Modern in-app browser markers (IABMV, Threads/Barcelona) |
| [monperrus/crawler-user-agents](https://github.com/monperrus/crawler-user-agents) | MIT | Search crawler, SEO, monitoring, link-preview bots |
| [omrilotan/isbot](https://github.com/omrilotan/isbot) | Unlicense | Generic automation / HTTP client tokens |

Vendor documentation from Google, Microsoft, Meta, TikTok, Snapchat, LinkedIn, Pinterest, OpenAI,
Anthropic, Perplexity, Apple, Amazon, Huawei, Mozilla and Chromium is cited per rule.

Copyleft projects — Snowplow referer-parser (GPL-3.0), Plausible (AGPL-3.0), ClearURLs (LGPL-3.0),
AdGuard (GPL-3.0), Neat-URL (GPL-2.0), Matomo device-detector (LGPL-3.0) — are **cited as prior art
only; no data from them is copied here**. If you want one of those tables, convert it yourself and
load it at runtime, at which point its licence is your call:

```rust
let attributor = Attributor::new().with_external_file("merged.json")?;
```

Full detail in [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md). The survey that compared all of
these is in [docs/oss-survey/](docs/oss-survey/).

## Updating the rules

All rules are data. Nothing in [`rules/`](rules/) is compiled into Rust logic; the files are embedded
with `include_str!` and parsed at startup, so a rule change is a JSON edit.

| File | Contents | How it is maintained |
|---|---|---|
| `rules/rules.json` | Click IDs, in-app UA tokens, `X-Requested-With` packages, referrer hosts, share markers, AI referrer paths, GA4 regexes, Mastodon instances, bot tokens and bot headers | Hand-edited |
| `rules/devices.json` | Device / OS / form-factor regexes, browser regexes, client-hint mappings, surface rules | Hand-edited |
| `rules/oss-referrers.json` | Matomo host table and per-engine search keyword params | Regenerated from the upstream CC0 list |

Every rule carries two mandatory fields, and a test (`combined_log_parse_and_all_rules_annotated`)
fails the build if either is missing:

- `src` — the documentation URL the fact came from.
- `verified` — the date it was checked against that URL, or `null` if it has not been.

To regenerate the Matomo-derived table, shallow-clone the upstream list into
`docs/oss-survey/src/matomo-org__searchengine-and-social-list/` and run:

```console
$ node reference/ts/build-oss-rules.mjs
```

To swap rules at runtime without rebuilding, use `Attributor::from_json(rules, devices, oss)`.

See [CONTRIBUTING.md](CONTRIBUTING.md) before opening a rule PR — the short version is that a new
rule needs a source URL and a verification date, and the source has to be CC0/MIT/Apache or vendor
documentation.

## Repository layout

```text
Cargo.toml             the crate: traffic-sources (lib traffic_sources, bin traffic-sources)
src/lib.rs             attribution engine, taxonomy, confidence model
src/main.rs            CLI
rules/*.json           all rule data, embedded at build time
tests/sourced.rs       integration tests; every user agent is copied verbatim from a cited source
examples/basic.rs      library quick start
docs/API.md            full API reference
docs/CRITIQUE.md       self-review: what was fixed, what is still unverified
docs/oss-survey/       survey of ~20 comparable OSS projects and their licences
reference/ts/          the earlier TypeScript prototype, kept for reference
```

### The TypeScript prototype

[`reference/ts/`](reference/ts/) holds the original implementation this crate grew out of. It is a
single-result attributor with a narrower taxonomy and no evidence model — useful for comparing
behaviour, not for production. It has no dependencies:

```console
$ cd reference/ts && node --experimental-strip-types --test attribution.test.ts
```

## Contributing

Bug reports, new rules and corrections to existing ones are all welcome. Please read
[CONTRIBUTING.md](CONTRIBUTING.md); the entire PR checklist is `cargo fmt`, `cargo clippy`,
`cargo test`, and a source URL plus verification date on anything you add to `rules/`.

## Licence

MIT © 2026 Tobias Lutke. See [LICENSE](LICENSE) and [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
