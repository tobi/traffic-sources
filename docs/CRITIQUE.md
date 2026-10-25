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

## Still unverified (verified = null in data)
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
