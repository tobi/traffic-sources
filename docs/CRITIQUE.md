# Critic loop — 24 Oct 2026 (America/Chicago)

A self-review of the rules and the engine: what was checked against current vendor documentation,
what was wrong and got fixed, and — in the last section — what is still **unverified**. Read that
last section before trusting a rule; it is the honest list of what this library guesses at.

Each rule in `rules/rules.json` and `rules/devices.json` carries `src` (doc URL) and `verified` (2026-10-24 = checked against that doc in this loop; `null` = not re-verified).

## Pass 1: review against current docs
| # | Finding | Status | Fix | Source |
|---|---|---|---|---|
| 1 | v2 said "Chrome is phasing out X-Requested-With" | **STALE** | WebView removal cancelled; header sent on all requests again; allowlist API deprecated/removed (WebView 142). Weight 0.75→0.85; Page note fixed | chromestatus.com/feature/5160086884843520; issuetracker.google.com/issues/447094379; androidx.webkit 1.15.0 notes |
| 2 | GA4 channel name | **FIXED** | "AI Assistant" (singular); medium `ai-assistant`, campaign `(ai-assistant)` now map to it; AI Overviews/AI Mode stay Organic Search | support.google.com/analytics/answer/9756891 |
| 3 | Bot list missing current agents | **FIXED** | Added OAI-AdsBot (ad_verifier), Claude-SearchBot, OAI-SearchBot, PerplexityBot, Meta-WebIndexer (ai_search_crawler), Google-Agent, Meta-ExternalFetcher, Perplexity-User (ai_user_fetch), Meta-ExternalAds; split into 9 classes | developers.openai.com/api/docs/bots; support.claude.com/en/articles/8896518; docs.perplexity.ai/docs/resources/perplexity-crawlers; developers.google.com/crawling/docs/crawlers-fetchers/google-user-triggered-fetchers; developers.facebook.com/docs/sharing/webmasters/web-crawlers/ |
| 4 | ChatGPT agent not detectable by UA | **ADDED** | `Signature-Agent: "https://chatgpt.com"` header (Web Bot Auth) → ai_user_fetch | OpenAI bots doc / Web Bot Auth |
| 5 | oppref (ChatGPT ads) | confirmed | — | help.openai.com/en/articles/20001409; developers.openai.com/ads/measurement-pixel |
| 6 | gad_source / gad_campaignid, gclid/gbraid/wbraid | confirmed | — | support.google.com/google-ads/answer/16193746, /3095550 |
| 7 | srsltid = organic shopping (Merchant Center) | confirmed | weak ID | support.google.com/merchants/answer/15191080 |
| 8 | msclkid, ttclid, li_fat_id, epik, ScCid/sccid, fbclid | confirmed | ScCid now case-insensitive | learn.microsoft.com msa-help; TikTok Events API; LinkedIn Conversions; Pinterest help; Snapchat help; Meta CAPI fbc doc |
| 9 | twclid, rdt_cid | secondary source only | kept, marked | leadtrackr.io (not vendor doc) |
| 10 | Bot token substring false positives (`Code`, `Dot`, `Muse`, `Trae` from ai.robots.txt) | **BUG** | word-boundary RegexSet, longest/documented token wins | — |
| 11 | No device taxonomy in v2 | **ADDED** | 31 device rules (console, TV, XR, car, wearable, e-reader, HarmonyOS NEXT, KaiOS) | uap-core tests (Apache-2.0); Meta Horizon browser specs; Amazon Fire TV UA doc; Huawei ArkWeb UA doc |
| 12 | Client hints ignored | **ADDED** | Sec-CH-UA-Platform/-Mobile/-Model/-Platform-Version/-Form-Factors override UA; Windows 11 = platform-version ≥13 | MDN Client hints; WICG UA-CH; learn.microsoft.com …/how-to-detect-win11 |
| 13 | UA reduction | **ADDED** | `os_version_frozen` for "Mac OS X 10_15_7" and "Android 10; K" | chromium.org/updates/ua-reduction |

## Pass 2: re-review of the crate
| # | Finding | Fix |
|---|---|---|
| 1 | Noisy-OR saturates: `twclid` + X UA gave paid 0.997 vs organic 0.963 (both source-only boosted) | conflict discount `P·(1−0.5·w)` from explicit UTM/click-ID; organic now 0.515 |
| 2 | Organic google.com referrer hurt `gclid` (separate hypothesis) | organic referrers are source-only evidence → gclid+google = 0.98 paid_search |
| 3 | Xbox Edge (EdgeHTML `Edge/20`) reported as Chrome | browser regex `Edge?/` |
| 4 | Regex compiled per request (8 s test run) | `OnceLock` literal regexes → 0.7 s |
| 5 | Weak IDs (fbclid) asserted organic | fbclid is now source-only: Meta docs say it's on ads *and* organic outbound links |
| 6 | Licences | shipped: Matomo (CC0), ai.robots.txt (MIT), uap-core-derived tokens (Apache-2.0), crawler-user-agents (MIT), isbot (Unlicense). GPL/AGPL/LGPL lists only via `--external` |

## Pass 3: reading the engine end to end
Found by reading `src/lib.rs` in full and probing each path with hand-built requests; each fix has a
regression test, and `tests/golden.rs` now pins every field of every result over 5,812 generated
requests and 45 log lines.

| # | Finding | Fix |
|---|---|---|
| 1 | `parse_combined_log` compiled its regex on every call, and share markers compiled two regexes per marker per request: 20k log lines took 5.5 s | compiled once; 134 ms (41×) |
| 2 | `sccid` double-counted: both `ScCid` and `sccid` rules fired, inflating 0.93 to 0.995 | click IDs match case-insensitively, duplicate removed, loader rejects case-insensitive collisions |
| 3 | Four bot tokens listed twice under different casing, so JSON order decided the class (and `exclude` for Meta-WebIndexer) | duplicates removed, loader rejects collisions |
| 4 | Seventeen bot operators were prose from ai.robots.txt ("unclear at this time.") and surfaced as `source` | slugs; the generator slugifies future imports |
| 5 | A prefetch marker suppressed the no-referrer analysis, so a prefetched cross-site navigation was `direct` (0.3) | the no-referrer analysis runs for prefetches too |
| 6 | `parse_combined_log` kept a `-` user agent, so UA-less log lines counted as human | `-` becomes `None` → automation |
| 7 | `utm_source` values ending in "ad"/"ads" were stripped before the category lookup: `threads` became `thre` → referral | the suffix is stripped only from sources that match the paid-source regex |
| 8 | The shipped search keyword table was never used | source-only `search_keyword` signal |
| 9 | Unknown channel / bot-class names in rules silently defaulted | loader error |

## Pass 4: zero dependencies
`serde`, `serde_json`, `regex` and `url` were replaced with code in the crate: a linear-time regex
engine (`src/pattern.rs`), a WHATWG URL subset (`src/url.rs`) and a JSON module (`src/json.rs`).
The bar was *identical results*, checked three ways:

- **Golden digest.** The human-readable digest of all ~5,900 golden results is byte-identical before
  and after, and `expected.txt` did not change.
- **Differential tests** (`difftest/`, run in CI). Every built-in pattern against the `regex` crate
  (`is_match`, `find` and every capture group) over the golden corpus plus edge cases, every
  golden and edge-case URL against the `url` crate, and a seeded fuzz run of 200,000 URLs and
  20,000 haystacks. They found five differences in the new URL parser before it landed: `^` must
  not be percent-encoded in paths; `file:` drive letters (`C|` → `C:`, the drive displacing the
  host, leading empty segments); fullwidth host characters; `xn--` labels must be valid punycode
  with an ASCII-only basic part; percent-decoded hosts that are not UTF-8 must fail. All are fixed
  and covered by unit tests.
- **CLI output** over 20k NDJSON requests and 20k log lines, compared as parsed JSON: identical
  apart from the new `reason` field on unparseable lines.

**Accepted differences** (asserted precisely by the fuzz test, so any new one fails CI):

| Input | `url` crate | This crate | Why |
|---|---|---|---|
| Non-special scheme with credentials and no host (`android-app://@`), or a non-numeric port (`foo://h:\`) | parses | rejects | The URL Standard says these are failures. |
| `xn--` labels that are valid punycode but not valid IDNA (`xn--1fban`), or the reverse | per UTS #46 tables | punycode check only | Full IDNA mapping and validity tables would be most of the crate. No real referrer host is affected. |

Also: `(?i)` folds ASCII only (no rule needs Unicode case folding), and patterns using look-around,
backreferences or `\p{…}` are rejected at load time rather than half-supported.

**Performance** (release build, 20k requests, same machine):

| | before | after |
|---|---|---|
| `Attributor::new` | 47 ms | 4 ms |
| `attribute` (NDJSON corpus) | 231 ms | 118 ms |
| `attribute` (access-log corpus) | 118 ms | 89 ms |
| `classify_bot` | 57 ms | 8 ms |
| `classify_client` | 67 ms | 42 ms |
| serialise results | 40 ms | 41 ms |
| parse NDJSON requests | 6 ms | 11 ms |

The gains come from a single Aho–Corasick pass over each rule list's required literals, which leaves
only the few rules that can possibly match to the matcher (the bot list alone has 249 entries).
After that come a bounded backtracker with a first-byte skip table, and the ASCII-lowercased
haystack being computed once per request rather than once per pattern.

## Pass 5: adversarial requests
The starting question was what happens to "direct" traffic (no `Referer`, no UTM, no click ID) when
the request carries frame headers or X's in-app user agent. From there the pass widened to conflicting
signals, wrapper chains, self-referrals, prefetch, contradictory Client Hints, android-app referrers,
malformed input and unusual devices. The new tests are:

- `tests/adversarial.rs`: 46 tests, with sourced user agents and a citation above each constant.
- `tests/invariants.rs`: 10 seeded property tests, 3,000 cases by default (`INVARIANT_CASES`). They
  passed at 200,000.
- The golden corpus gains a Fetch Metadata grid of 2,400 rows (dest × site × mode/user × client ×
  referrer), for 8,227 requests in total.

### Decisions
**Frames.** `Sec-Fetch-Dest` and `Sec-Fetch-Site` are set by the browser and cannot be changed by
page script, so they are trusted over the user agent.

- An `iframe`/`frame`/`embed`/`object` load is not a visit. It is never a new touch.
- With no referrer, a cross-site frame is `referral` / `(embedding page)` at 0.45. Someone else's page
  embeds ours, and the referrer was withheld by `referrerpolicy` or `Referrer-Policy`.
- A `same-site`/`same-origin` frame is `internal` / `(self)` at 0.6.
- `none` or a missing site is `referral` at 0.3, because there is little to go on.
- None of these are `direct` or `dark_social`. Those describe a person arriving, and an embedded frame
  is not a person arriving.
- `Sec-Fetch-Mode` and `Sec-Fetch-User` do not change the result. Mode is `navigate` for frames anyway.
  `Sec-Fetch-User` says that a person activated the navigation. It says nothing about where the
  navigation came from.
- Other non-document destinations (`script`, `image` and so on) are `subresource` and are not touches.

**X's in-app browser.** The `Twitter for iPhone`/`Twitter for iPad` tokens, `TwitterAndroid` with
`; wv`, and `X-Requested-With: com.twitter.android` are first-party app markers. X opens links
without a referrer, so with no other evidence the visit is `organic_social` / `x` at 0.85, a click
and a new touch, on surface `webview_iab`. UA plus XRW is 0.978.

The app tells us where the person is, not that the site was not reached from within the app.
Fetch Metadata still wins:

- `same-origin` is `internal`, because the person is already browsing our site inside X.
- An iframe keeps `x` first, but as an `embed` that is not a touch.

When a full facebook UTM set sits on a link opened in X, facebook ranks first and x second. The UTM
names the campaign, and the app is most likely where a copied link was pasted. With only
`utm_source=facebook`, x stays first.

**Unfurlers.** Twitterbot, facebookexternalhit, Slackbot and the others are `bot` / `link_preview`,
with `exclude` set and surface `bot`, whatever else the request carries. A preview fetch with frame
or UTM headers is still a preview fetch.

### Findings and fixes
| # | Finding | Fix |
|---|---|---|
| 1 | `Sec-Fetch-Dest` was ignored: a cross-site iframe with no referrer was dark social and a new touch | `Interaction::Embed`; referral `(embedding page)` 0.45 cross-site, internal 0.6 same-site, never a touch |
| 2 | An in-app UA token or XRW overrode `Sec-Fetch-Site: same-origin`, so every page view inside X was a fresh social click | same-origin/same-site with no referrer is internal and wins |
| 3 | `new_touch` was true for same-origin requests with no referrer | touch requires a cross-site or unknown page load |
| 4 | Subresource requests (`script`, `image`, …) counted as visits | `Interaction::Subresource`, not a touch |
| 5 | Bots never got surface `bot` (Discordbot was `webview_iab`, from its UA's in-app token) | a bot match sets surface `bot` and clears `app` |
| 6 | Same-origin requests also carried a meaningless `direct` 0.3 candidate | dropped |
| 7 | Whitespace-only XRW became `organic_social` / `" "`; quote and junk values became app names | XRW must be a package name (`a.b[.c]`) and not a browser package |
| 8 | UTM values were not trimmed: `utm_source=%20facebook%20` was `paid_other` | values trimmed, and blank counts as absent; header values are trimmed too |
| 9 | A self referrer with a trailing dot (`shop.example.`) was a referral | host's trailing dot dropped |
| 10 | Apex vs `www.` self host was a referral | self hosts compared without `www.` |
| 11 | `utm_medium=email` with a t.co referrer was `organic_social`: the referrer's category overrode the UTM | GA4 order: `utm_source` decides, and the referrer stands in only when it is missing |
| 12 | `utm_source=facebook` with a google referrer was `organic_search` / facebook | same fix |
| 13 | An empty `android-app://` referrer produced a custom tab with app `""` | android-app hosts must be package names |
| 14 | The Gmail app (`android-app://com.google.android.gm`) with `utm_medium=email` was `organic_search` | the referrer's mapped app name (gmail) is categorised, not the package's host |
| 15 | With only `utm_medium`, the source was the raw referrer host or app at random (`mastodon.social` vs `mastodon`; outlook vs facebook) | one resolution order: utm_source, then the referrer's rule name, then the app |
| 16 | Chromium Client Hints overrode Safari, Firefox, KaiOS and console UAs (a spoofing extension or proxy sends both) | hints are used only when the UA is empty or Chromium |
| 17 | Mail-client UAs (Outlook, Thunderbird) with no referrer were plain direct | `email` 0.60 (`EMAIL_CLIENT_NO_REFERRER`); app set |
| 18 | Gmail's source was `mail.google.com` | `gmail` |
| 19 | GPTBot got operator `other` (the `OpenAI` token matched the URL inside its UA first); Claude Code was misclassified | tie-break on verified, token length, then earliest match position |
| 20 | 53 AI-crawler operators were `other` | named vendors |
| 21 | Missing bots: bare `WhatsApp` (the rule required `WhatsApp/`), vkShare, BlueskyPreviewBot, Mastodon, Google-InspectionTool, SnapchatAds | added |

### Known limitations (pinned by tests, not fixed)
- A bare `Viber` preview UA collides with Viber's in-app token and is read as the in-app browser.
- X's native API client UA (`TwitterAndroid/2.0`, no `Mozilla/`) is reported as `webview_iab`. It
  never loads pages, so this only matters for logs.
- A `same-origin` request with UTM still ranks the UTM channel first, but is not a new touch. GA4
  would start a new session.
- A forged referrer that contradicts `Sec-Fetch-Site` keeps the referrer. A self referrer with
  `cross-site` is a payment-provider round trip, so `internal` is correct there.
- Bot `src` citations are per class (every vendor doc behind the class), not per rule. Twitterbot's
  list does not include X's own documentation.

## Still unverified (verified = null in data)
- New bot rules: vkShare, BlueskyPreviewBot, Mastodon and SnapchatAds. The tokens come from
  crawler-user-agents (MIT) and have not been checked against each vendor's docs.
- iMessage's link-preview UA (a Facebook + Twitter UA pair); Viber's preview UA.
- UA tokens: Pico browser, Android Automotive, Wear OS, Echo Show (AEO*), current Tesla browser (only legacy QtCarBrowser sample), ChatGPT iOS in-app browser token.
- Vision Pro: Safari sends a Mac UA; not separable (enum value exists, never emitted).
- SFSafariViewController, App Clips, Instant Apps: indistinguishable from Safari/Chrome by headers.
- Android tablet = Android without "Mobile" (convention, not re-verified).
- PS5 / Xbox Series X samples from third-party UA sites, not vendor docs.
- Instagram iOS `Safari/604.1` change: secondary source (dev.to).
- X referrer behaviour (t.co vs none) and share markers (s=, t=, si=) — re-derived from ClearURLs (LGPL, cited only).
- Most of the 76 curated referrer hosts and 31 in-app UA tokens (other than X, Facebook, Instagram) are not re-checked against vendor docs this pass.
- Search crawler / SEO / monitoring tokens: from MIT lists, not each vendor doc.
- 2025-26 commits in uap-core / inapp-spy not fully reviewed.
