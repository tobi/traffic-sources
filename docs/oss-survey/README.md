# OSS survey: traffic source / channel / medium classification

A survey of the comparable open-source projects, done 2026-10-24, which is where the rule data in
[`../../rules/`](../../rules/) came from and which decided what could and could not be shipped.

Each project below was shallow-cloned into `src/` as a read-only reference, and `build.py` merges
them into `data/merged.json` and `data/merged.yaml`:

```console
cd docs/oss-survey
git clone --depth 1 https://github.com/<owner>/<repo> src/<owner>__<repo>   # per project below
python3 build.py                                                            # needs PyYAML
```

**`src/` and `data/` are gitignored and must stay that way.** Several of the projects cloned here
are GPL, AGPL or LGPL, so `data/merged.json` is copyleft-contaminated by construction. It is
*never* vendored into `rules/`. If you want it, build it locally and load it at runtime:

```console
traffic-sources --external docs/oss-survey/data/merged.json < access.log
```

See [`../../THIRD-PARTY-NOTICES.md`](../../THIRD-PARTY-NOTICES.md) for which of these projects are
actually copied from (CC0/MIT/Apache/Unlicense only) and which are cited as prior art.

| Project | License | Last commit | Covers | Data paths | Quality |
|---|---|---|---|---|---|
| snowplow-referer-parser/referer-parser | GPLv3 (data derived from Matomo) | 2026-07-30 | referrer domain → source + medium (search/social/email/paid/unknown), search query params | `resources/referers.yml`, `referer-tests.json` | De-facto standard; ~2.4k domains; slow updates, weak on AI/new socials |
| matomo-org/searchengine-and-social-list | **CC0** | 2026-09-23 | search engines (with keyword params), socials, **AI assistants** | `SearchEngines.yml`, `Socials.yml`, `AIAssistants.yml` | Best permissive source; actively maintained; has AI list |
| matomo-org/device-detector | LGPL-3.0 | 2026-10-20 | UA → device/OS/browser/bot, many in-app/app regexes | `regexes/client/mobile_apps.yml`, `regexes/bots.yml` | Most exhaustive UA regex set |
| plausible/analytics | AGPL-3.0 (code); csv is Google's published list | 2026-10-23 | GA4-style channel waterfall + AI Assistants channel, paid utm_source list, gclid/msclkid | `lib/plausible/ingestion/acquisition.ex`, `source.ex`, `priv/ga4-source-categories.csv`, `priv/custom_sources.json` | Best reference implementation of GA4 channel grouping |
| umami-software/umami | MIT | 2026-10-23 | channel buckets, paid param list, search/social/email/shopping/video domain lists | `src/lib/constants.ts` (PAID_AD_PARAMS, *_DOMAINS, GROUPED_DOMAINS) | Simple substring matching; useful, permissive |
| PostHog/posthog-js | MIT (repo-level, ee excluded) | 2026-10-23 | campaign + click-ID params captured (incl. `oppref` OpenAI ads, `_kx`, `gad_source`) | `packages/browser-common/src/utils/event-utils.ts` | Best up-to-date click-ID list. Server channel_type logic lives in posthog main repo (HogQL `channel_type`, MIT parts) — mirrors GA4 |
| arp242/goatcounter | EUPL-1.2 (slightly modified) | 2026-09-28 | ref cleanup, ref grouping, `ref`/`utm_source` param | `hit.go`, `ref.go` | Small but pragmatic (android-app:// handling) |
| ua-parser/uap-core | Apache-2.0 | 2026-08-24 | UA regexes incl. Facebook/Instagram/etc apps, bots | `regexes.yaml` | Solid, multi-language ports |
| omrilotan/isbot | Unlicense | 2026-10-01 | single bot regex | `src/patterns.json` | Excellent, low false positives |
| monperrus/crawler-user-agents | MIT | 2026-10-24 | bot UA patterns + instances + URLs | `crawler-user-agents.json` | Large, well maintained |
| ai-robots-txt/ai.robots.txt | MIT | 2026-10-24 | 200 AI crawler/agent UAs with operator & function | `robots.json` | Best open AI-bot list |
| Dark Visitors (darkvisitors.com) | proprietary/API | live | agent taxonomy (AI data scraper / AI assistant / AI search crawler / undocumented agent) | n/a | Use taxonomy, not data |
| shalanah/inapp-spy | MIT | 2026-06-13 | in-app browser detection (+ SFSafariViewController heuristics), modern UAs incl. IABMV, Threads=Barcelona | `src/regexAppName.ts`, `src/tests/*.ts` | Best current in-app detector; real test UAs |
| f2etw/detect-inapp | MIT | 2022-06-16 | in-app detection | `src/` | Stale |
| bowser-js/bowser | MIT | 2026-09-27 | browser/engine parse | `src/parser-browsers.js` | Fine for engine; not in-app aware |
| ClearURLs/Rules | LGPL-3.0 | 2026-03-25 | per-site tracking params (574 patterns) | `data.min.json` | Very broad; regex patterns; some site-specific |
| Smile4ever/Neat-URL | GPL-2.0 | 2026-10-24 | tracking params with domain scoping | `data/default-params-by-category.json` | Good complement |
| AdguardTeam AdguardFilters (URL Tracking filter) | GPL-3.0 | 2026-10-24 | `$removeparam` rules | `TrackParamFilter/sections/*.txt` | Largest, most current param list |
| GA4 default channel group + source categories (Google) | Google docs (Plausible ships copy) | — | canonical channel rules, ~820 source→category | support.google.com/analytics/answer/9756891 & 9164331 | Canonical; no AI channel |
Also notable: Segment analytics.js/RudderStack (MIT/Elastic) only capture `context.campaign` + referrer, no classification; Open Web Analytics (GPL) has dated search-engine list; Fathom closed; Ackee (MIT) has no channel logic.

## Licensing of merged data
`data/merged.*` mixes facts from GPLv3 (referers.yml), CC0 (Matomo lists), MIT (Plausible custom_sources is AGPL — treat as reference), LGPL (ClearURLs). Domain/param names are largely factual, but **if code ships the merged file, prefer the CC0/MIT subsets** (Matomo lists, umami, PostHog, ai.robots.txt, inapp-spy) or keep the GPL-derived data as an external loadable file. Each referrer entry records `from:` provenance.

## Merged data contents
- `referrers` (2,970 domains → source/medium/provenance), `ga4_source_categories` (819), `click_ids` (49 with platform & paid/organic notes), `clearurls_tracking_param_patterns` (574), `ai_bots` (200 with operator/function), `ai_referrers` (32), `inapp_ua_tokens` (29 regexes).

## What the main attribution effort may miss (top additions)
1. **AI Assistants as its own channel** (Plausible does this; GA4 doesn't): chatgpt.com, perplexity.ai, claude.ai, gemini.google.com, copilot.microsoft.com, grok.com, chat.deepseek.com, meta.ai, duck.ai, kagi assistant, poe.com, chat.mistral.ai; plus **`utm_source=chatgpt.com`** auto-tag and **`oppref`** (OpenAI ads click ID → Paid AI).
2. **Distinguish AI fetchers vs AI crawlers** from ai.robots.txt `function`: ChatGPT-User, Perplexity-User, Claude-User, MistralAI-User = user-initiated visits (count as AI referral-ish human proxy), vs GPTBot/ClaudeBot/CCBot = crawlers (exclude).
3. **`srsltid` = Google Merchant organic listing**, not paid (common misclassification). `fbclid` appears on organic Meta shares too — not a paid signal alone; Meta paid uses `utm_*`/`ad_id`. `igshid/igsh`, `si`, X `s=`/`t=` are share markers (organic social).
4. **GA4 waterfall order** incl. Cross-network (`campaign` contains cross-network), Paid Shopping (shopping-campaign regex `^(.*(([^a-df-z]|^)shop|shopping).*)$`), Display mediums (display/banner/expandable/interstitial/cpm), SMS, Mobile Push (medium ends with "push", or contains "mobile"/"notification", or source=firebase), Audio, Affiliates; `paid_medium` regex `^(.*cp.*|ppc|retargeting|paid.*)$`.
5. **Paid-by-utm_source** list (Plausible): any utm_source ending in `ads`/`ad` + `adwords` (e.g. `fb_ad`, `yt-ads`) → paid regardless of medium.
6. **Click IDs often forgotten**: gbraid/wbraid (iOS ATT Google), gad_source/gad_campaignid, dclid, sccid/ScCid, epik, rdt_cid, qclid, li_fat_id, yclid, tblci/ob_click_id (Taboola/Outbrain native → Display/Paid Other), irclickid/cjevent/awc/ranMID (affiliates), `_kx`/mc_eid/_hsenc/mkt_tok (email).
7. **App-scheme referrers**: `android-app://com.google.android.gm` (Gmail), `android-app://com.reddit.frontpage`, `android-app://org.joinmastodon.android`, `android-app://com.slack`, `android-app://com.google.android.googlequicksearchbox` (Google app → organic search).
8. **Wrapper/redirect domains**: l.facebook.com, lm.facebook.com, l.instagram.com, l.threads.com, t.co, lnkd.in, out.reddit.com, away.vk.com, ntp.msn.com (Edge new tab → Bing), statics.teams.cdn.office.net (Teams), *.officeapps.live.com, com.google.android.gm, googleadservices, syndicatedsearch.goog (Google AFS → paid search).
9. **Mastodon/fediverse** long tail (dozens of instances) and Bluesky (bsky.app, go.bsky.app), Lemmy, Threads.
10. **In-app tokens beyond the big ones**: `IABMV/1` (new Meta IAB), `Barcelona` (Threads), `WAiOS/WA4A`, `GSA/` (Google app), `BytedanceWebview`/`trill_`/`aweme`, `KAKAOTALK`, `NAVER(inapp`, `YJApp`, `Zalo`, `Snapchat/… (like Safari … panda)`, Android `; wv)`; SFSafariViewController/Chrome Custom Tabs are **undetectable by UA** — rely on referrer/click IDs.
11. **Search keyword params** per engine (Matomo/Snowplow `parameters:`) — useful for Organic vs Paid when `q=` present on referrer.
