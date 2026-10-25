# traffic-sources — API (v0.3.0)

Crate: `traffic-sources` (lib `traffic_sources`, bin `traffic-sources`). Rust 2021, deps: serde, serde_json, regex, url.

## Functions

```rust
impl Attributor {
    pub fn new() -> Self;                                             // embedded rules (include_str!)
    pub fn from_json(rules: &str, devices: &str, oss: &str) -> Result<Self, RuleError>;
    pub fn with_external_referrers(self, json: &str) -> Result<Self, RuleError>; // e.g. GPL lists, not shipped
    pub fn with_external_file(self, path: &str) -> Result<Self, RuleError>;
    pub fn attribute(&self, req: &Request) -> Attribution;            // all candidates, ranked
    pub fn classify_client(&self, req: &Request) -> Client;           // OS / form factor / surface / app
    pub fn classify_bot(&self, req: &Request) -> Option<Bot>;
    pub fn ga4_channel(&self, source: &str, medium: &str, campaign: &str, ref_host: &str) -> (Channel, bool);
}
impl Attribution { pub fn top(&self) -> &Candidate; }
pub fn parse_combined_log(line: &str, default_host: &str) -> Option<Request>;
pub mod weights { UTM_FULL=0.95, UTM_PARTIAL=0.80, CLICK_ID=0.93, WEAK_CLICK_ID=0.70, UA_TOKEN=0.85,
  X_REQUESTED_WITH=0.85, SHARE_MARKER=0.70, ANDROID_APP_REF=0.80, REFERRER=0.75, AI_PATH=0.80,
  OSS_LIST=0.65, EXTERNAL_LIST=0.60, UNKNOWN_REFERRER=0.60, INTERNAL=0.90, DIRECT=0.50,
  DARK_SOCIAL_SECFETCH=0.45, DARK_SOCIAL_WEBVIEW=0.35, BOT_DOCUMENTED=0.97, BOT_GENERIC=0.85,
  FAMILY_FACTOR=0.80, CONFLICT_FACTOR=0.50 }
```
`Attributor` is `Send + Sync`; build once.

## Types
- `Request { url, referer?, user_agent?, headers: BTreeMap (case-insensitive), self_hosts[], self_android_packages[] }`
- `Attribution { candidates: Vec<Candidate>, client: Client, bot?: Bot, is_prefetch, new_touch, evidence: Vec<Evidence> }`
- `Candidate { channel, source, medium, campaign?, paid: Paid, interaction: Interaction, confidence: 0..=1, share, evidence: Vec<usize> }`
- `Evidence { kind: SignalKind, signal, weight, channel?, source?, rule_src? (doc URL), verified? (date) }`
- `Client { os: Os, os_version?, os_version_frozen, form_factor: FormFactor, device?, browser?, surface: Surface, app?, client_hints_used, confidence, rule_src? }`
- `Bot { class: BotClass, token, operator, exclude, rule_src? }`

## Taxonomy (snake_case in JSON)
- **Channel**: direct, organic_search, paid_search, organic_social, paid_social, organic_video, paid_video, organic_shopping, paid_shopping, display, cross_network, paid_other, ai_assistant, paid_ai, email, affiliate, referral, audio, sms, mobile_push, internal, dark_social, bot, unknown
- **Paid**: paid, organic, unknown
- **Interaction**: click, share, ad_click, navigation, internal, prefetch, preview_unfurl, ai_user_fetch, crawl, ad_verification, monitoring, automation, unknown
- **Surface**: standalone_browser, webview_iab, safari_view_controller*, custom_tab, pwa, twa, app_clip*, instant_app*, amp_cache, email_client, webmail, native_app*, bot, unknown  (*= in the enum but not detectable from headers; SFSafariViewController sends Safari's UA)
- **Os**: ios, ipados, android, windows, macos, linux, chromeos, harmonyos, kaios, tvos, tizen, webos, fire_os, roku, android_tv, cast_os, playstation, xbox, nintendo_switch, steamos, meta_horizon_os, visionos*, pico_os, wear_os, watchos, android_automotive, tesla, fuchsia, unknown
- **FormFactor**: phone, tablet, desktop, tv, console, xr_headset, wearable, car, e_reader, smart_display, embedded, unknown
- **BotClass**: search_crawler, ai_crawler, ai_search_crawler, ai_user_fetch, ad_verifier, link_preview, seo_tool, monitoring, prefetch, automation

## Confidence
Each evidence item i has weight w_i and names a channel and/or source. Candidate (channel c, source s):
`P = 1 − Π(1 − w_i · f_i)` over compatible evidence (f=1 same source, 0.8 same family, e.g. Meta), then
for each explicit UTM/click-ID item tagging the same source with a different channel:
`P ← P · (1 − 0.5 · w_j)`. Source-only evidence (UA token, X-Requested-With, organic referrer) supports every channel of that source.
`confidence` is a belief per hypothesis; `share` is normalised over candidates. Ranking: confidence desc, then paid first.

## CLI
```
traffic-sources [--host shop.example] [--external merged.json] [--top] < access.log > out.ndjson
```
stdin: combined log lines or NDJSON `Request`; stdout: one JSON object per line (`{"url","attribution"}` or with --top `{"url","top","client","bot"}`).

## Example
Input:
```json
{"url":"https://shop.example/p?twclid=abc","referer":"https://t.co/","user_agent":"Mozilla/5.0 (iPhone; CPU iPhone OS 18_6_2 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/22G100 Twitter for iPhone/12.31.2"}
```
Output:
```json
{
    "attribution": {
        "candidates": [
            {
                "channel": "paid_social",
                "confidence": 0.997,
                "evidence": [
                    0,
                    1,
                    2
                ],
                "interaction": "ad_click",
                "medium": "paid_social",
                "paid": "paid",
                "share": 0.659,
                "source": "x"
            },
            {
                "channel": "organic_social",
                "confidence": 0.515,
                "evidence": [
                    1,
                    2
                ],
                "interaction": "click",
                "medium": "social",
                "paid": "organic",
                "share": 0.341,
                "source": "x"
            }
        ],
        "client": {
            "app": "x",
            "client_hints_used": false,
            "confidence": 0.85,
            "device": "Apple",
            "form_factor": "phone",
            "os": "ios",
            "os_version": "18.6.2",
            "os_version_frozen": false,
            "rule_src": "https://github.com/ua-parser/uap-core/tree/master/tests (Apache-2.0)",
            "surface": "webview_iab"
        },
        "evidence": [
            {
                "channel": "paid_social",
                "kind": "click_id",
                "rule_src": "https://leadtrackr.io/learn/advertising-click-ids",
                "signal": "click_id:twclid",
                "source": "x",
                "verified": "2026-10-24",
                "weight": 0.93
            },
            {
                "kind": "ua_token",
                "rule_src": "https://github.com/ua-parser/uap-core/blob/master/regexes.yaml + https://docs.uaparser.dev/info/browser/name/twitter.html",
                "signal": "ua_token:x",
                "source": "x",
                "verified": "2026-10-24",
                "weight": 0.85
            },
            {
                "kind": "referrer",
                "rule_src": "https://github.com/matomo-org/searchengine-and-social-list (CC0) + curated",
                "signal": "referer:t.co",
                "source": "x",
                "weight": 0.75
            }
        ],
        "is_prefetch": false,
        "new_touch": true
    },
    "url": "https://shop.example/p?twclid=abc"
}
```
