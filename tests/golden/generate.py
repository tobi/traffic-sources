#!/usr/bin/env python3
"""Generate the differential-test corpus in tests/golden/.

Deterministic: rerunning produces byte-identical files. The corpus is built from the rule data
itself (every bot token, click ID, OSS host, Mastodon instance, X-Requested-With package, ...)
plus hand-picked real user agents and adversarial inputs, so that a change in behaviour anywhere
in the engine shows up as a changed digest in `expected.txt`.

    python3 tests/golden/generate.py        # rewrite requests.tsv and access.log
    UPDATE_GOLDEN=1 cargo test --test golden   # re-bless expected.txt after an intended change

requests.tsv columns (tab-separated; empty = absent):
    url, referer, user_agent, self_hosts (comma-separated), then any number of `name: value`
    header columns.
"""
import json
import os
import random

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.join(HERE, "..", "..")
R = json.load(open(os.path.join(ROOT, "rules", "rules.json")))
O = json.load(open(os.path.join(ROOT, "rules", "oss-referrers.json")))

rng = random.Random(20261024)

CHROME_MAC = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36"
CHROME_WIN = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36"
CHROME_AND = "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Mobile Safari/537.36"
SAFARI_IOS = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.6 Mobile/15E148 Safari/604.1"
FIREFOX = "Mozilla/5.0 (X11; Linux x86_64; rv:144.0) Gecko/20100101 Firefox/144.0"

# Real user agents, copied verbatim from tests/sourced.rs and its cited sources.
REAL_UAS = [
    CHROME_MAC, CHROME_WIN, CHROME_AND, SAFARI_IOS, FIREFOX,
    "Mozilla/5.0 (Linux; Android 16; 23078PND5G Build/BP2A.250605.031.A3; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/153.0.8010.39 Mobile Safari/537.36 TwitterAndroid/12.32.0-prod.01 (312320001-r-01) 23078PND5G/16 (Xiaomi;23078PND5G;Xiaomi;corot_global;0;;1;2015)",
    "Mozilla/5.0 (iPhone; CPU iPhone OS 18_6_2 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/22G100 Twitter for iPhone/12.31.2",
    "Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148 Instagram 395.1.4.56.91",
    "Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148 [FBAN/FBIOS;FBAV/470.0.0.40.108;FBBV/600000000;FBDV/iPhone15,2;FBMD/iPhone;FBSN/iOS;FBSV/17.5;FBSS/3;FBCR/;FBID/phone;FBLC/en_US;FBOP/80]",
    "Mozilla/5.0 (Linux; Android 14; SM-S918B Build/UP1A.231005.007; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/120.0.6099.230 Mobile Safari/537.36 [FB_IAB/FB4A;FBAV/447.0.0.37.106;]",
    "Mozilla/5.0 (iPhone; CPU iPhone OS 17_4 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148 Barcelona 330.0.0.27.110",
    "Mozilla/5.0 (Linux; Android 13; Pixel 7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/116.0.0.0 Mobile Safari/537.36 musical_ly_2023109030 JsSdk/1.0 NetType/WIFI Channel/googleplay AppName/musical_ly app_version/31.9.3 ByteLocale/en ByteFullLocale/en Region/US",
    "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148 MicroMessenger/8.0.42(0x18002a2f) NetType/WIFI Language/zh_CN",
    "Mozilla/5.0 (Linux; Android 11; KFTRWI) AppleWebKit/537.36 (KHTML, like Gecko) Silk/120.4.1 like Chrome/120.0.6099.230 Safari/537.36",
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) OculusBrowser/35.0.0.6.68 SamsungBrowser/4.0 Chrome/126.0.6478.122 VR Safari/537.36",
    "Mozilla/5.0 (PlayStation 5 3.11) AppleWebKit/605.1.15 (KHTML, like Gecko)",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; Xbox; Xbox One) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/70.0.3538.102 Safari/537.36 Edge/18.19041",
    "Mozilla/5.0 (Nintendo Switch; WifiWebAuthApplet) AppleWebKit/606.4 (KHTML, like Gecko) NF/6.0.1.15.4 NintendoBrowser/5.1.0.20393",
    "Mozilla/5.0 (Linux; Android 9; AFTMM) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/89.0.4389.90 Mobile Safari/537.36",
    "Mozilla/5.0 (Web0S; Linux/SmartTV) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/79.0.3945.79 Safari/537.36 WebAppManager",
    "Mozilla/5.0 (SMART-TV; LINUX; Tizen 6.0) AppleWebKit/537.36 (KHTML, like Gecko) 76.0.3809.146/6.0 TV Safari/537.36",
    "Mozilla/5.0 (Phone; OpenHarmony 5.0) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/114.0.0.0 Safari/537.36 ArkWeb/4.1.6.1 Mobile",
    "Mozilla/5.0 (Mobile; LYF/F300B/LYF-F300B-001-01-15-130718-i;Android; rv:48.0) Gecko/48.0 Firefox/48.0 KAIOS/2.5",
    "Mozilla/5.0 (X11; CrOS x86_64 14541.0.0) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36",
    "Mozilla/5.0 (X11; Linux) AppleWebKit/534.34 (KHTML, like Gecko) QtCarBrowser Safari/534.34",
    "Mozilla/5.0 (iPad; CPU OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1",
    "Mozilla/5.0 (Linux; Android 14; SM-X710) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36",
    "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) GSA/340.0.689826392 Mobile/15E148 Safari/604.1",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36 Edg/141.0.0.0",
    "Microsoft Office/16.0 (Windows NT 10.0; Microsoft Outlook 16.0.17928; Pro)",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:128.0) Gecko/20100101 Thunderbird/128.3.1",
]

URL = "https://shop.example/p"
rows = []


def row(url=URL, referer="", ua=CHROME_MAC, self_hosts="", **headers):
    hs = [f"{k.replace('_', '-')}: {v}" for k, v in sorted(headers.items())]
    for f in [url, referer, ua, self_hosts, *hs]:
        assert "\t" not in f and "\n" not in f, f
    rows.append("\t".join([url, referer, ua, self_hosts, *hs]))


# --- plain user agents, with and without referrers
for ua in REAL_UAS:
    row(ua=ua)
    row(ua=ua, referer="https://t.co/")
    row(ua=ua, sec_fetch_site="cross-site")

# --- every bot token, embedded the way bots actually send them, plus near misses
for b in R["botRules"]:
    t = b["token"]
    row(ua=f"Mozilla/5.0 (compatible; {t}/1.0; +https://example.com/bot)")
    row(ua=f"{t}")
    row(ua=f"Mozilla/5.0 (compatible; X{t}Y/1.0)")  # must not match: no word boundary
for b in R["botHeaders"]:
    row(**{b["header"].replace("-", "_"): b["contains"]})

# --- every click ID, alone, upper-cased, alongside a referrer, and on an internal navigation
for c in R["clickIds"]:
    p = c["param"]
    row(url=f"{URL}?{p}=abc")
    row(url=f"{URL}?{p.upper()}=abc")
    row(url=f"{URL}?{p}=abc", referer="https://www.google.com/")
    row(url=f"{URL}?{p}=abc", referer="https://www.youtube.com/watch?v=x")
    row(url=f"{URL}?{p}=abc", referer="https://shop.example/cart")

# --- every OSS host as a referrer; keyword hosts also with a query
kw = O.get("keywordParams", {})
for h, v in sorted(O["hosts"].items()):
    host = h if "." in h else h + ".com"
    row(referer=f"https://{host}/")
    if h in kw:
        row(referer=f"https://{host}/search?{kw[h][0]}=running+shoes")
        row(referer=f"https://{host}/search?{kw[h][0]}=")

# --- curated referrer hosts and wrappers
HOSTS = [
    "www.google.com", "google.co.uk", "mail.google.com", "docs.google.com", "drive.google.com",
    "www.google.com/aclk?sa=l", "googleadservices.com", "syndicatedsearch.goog", "www.bing.com",
    "ntp.msn.com", "duckduckgo.com", "search.yahoo.com", "yandex.ru", "www.baidu.com",
    "l.facebook.com", "lm.facebook.com", "m.facebook.com", "l.instagram.com", "l.threads.com",
    "t.co", "x.com", "twitter.com", "lnkd.in", "www.linkedin.com", "out.reddit.com",
    "www.reddit.com", "away.vk.com", "www.tiktok.com", "www.pinterest.com", "bsky.app",
    "go.bsky.app", "www.youtube.com", "m.youtube.com", "youtu.be", "open.spotify.com",
    "chatgpt.com", "chat.openai.com", "perplexity.ai", "www.perplexity.ai", "claude.ai",
    "gemini.google.com", "copilot.microsoft.com", "grok.com", "x.com/i/grok", "chat.deepseek.com",
    "outlook.live.com", "outlook.office.com", "mail.yahoo.com", "mail.proton.me",
    "statics.teams.cdn.office.net", "www-example-com.cdn.ampproject.org", "news.ycombinator.com",
    "www.amazon.com", "www.ebay.de", "shopping.google.com", "www.twitch.tv", "kick.com",
    "unknown-blog.example", "sub.shop.example", "shop.example",
]
for h in HOSTS:
    row(referer=f"https://{h}" + ("" if "/" in h else "/"))
    row(referer=f"https://{h}" + ("" if "/" in h else "/"), ua=SAFARI_IOS)
for a in R.get("aiPathReferrers", []):
    for path in ["/i/grok", "/i/grok?conversation=1", "/search?q=x", "/"]:
        row(referer=f"https://{a['host']}{path}")
for m in sorted(R.get("mastodonInstances", [])):
    row(referer=f"https://{m}/@someone/123")

# --- X-Requested-With: every known package, browsers, and android-app referrers
for pkg in sorted(R["xRequestedWith"]["apps"]):
    row(ua=CHROME_AND, x_requested_with=pkg)
    row(ua=CHROME_AND, referer=f"android-app://{pkg}/")
    row(ua=CHROME_AND, referer=f"android-app://{pkg}/", self_hosts="shop.example")
for pkg in ["com.android.chrome", "org.mozilla.firefox", "XMLHttpRequest", "com.example.unknownapp"]:
    row(ua=CHROME_AND, x_requested_with=pkg)
row(ua=CHROME_AND, referer="android-app://com.example.mytwa/", self_hosts="shop.example")

# --- UTM combinations through the GA4 waterfall
SOURCES = ["google", "bing", "facebook", "fb", "ig", "instagram", "threads", "x", "twitter", "tiktok",
           "linkedin", "youtube", "chatgpt.com", "perplexity", "newsletter", "klaviyo", "firebase",
           "sms", "fb_ad", "yt-ads", "adwords", "nomads", "adidas", "(direct)", "amazon", "spotify",
           "partner-site", "", "Google", "FACEBOOK"]
MEDIUMS = ["", "cpc", "ppc", "paid_social", "social", "organic", "email", "e-mail", "affiliate",
           "display", "banner", "cpm", "referral", "audio", "sms", "push", "mobile", "notification",
           "video", "ai-assistant", "(none)", "retargeting", "paidsearch", "app"]
CAMPAIGNS = ["", "spring_shop", "cross-network", "(ai-assistant)", "brand"]
for s in SOURCES:
    for m in MEDIUMS:
        c = CAMPAIGNS[(len(s) + len(m)) % len(CAMPAIGNS)]
        q = "&".join(f"{k}={v}" for k, v in [("utm_source", s), ("utm_medium", m), ("utm_campaign", c)] if v)
        if q:
            row(url=f"{URL}?{q}")
for s in SOURCES[:10]:
    row(url=f"{URL}?utm_source={s}&utm_medium=cpc", ua=REAL_UAS[7])
    row(url=f"{URL}?utm_source={s}&gclid=1", referer="https://www.google.com/")
    row(url=f"{URL}?UTM_SOURCE={s}&Utm_Medium=social")

# --- share markers
for q, ref in [("si=abc", "https://www.youtube.com/"), ("si=abc", "https://open.spotify.com/"),
               ("si=abc", ""), ("s=20", "https://t.co/"), ("s=20&t=xyz", ""), ("s=20", ""),
               ("s=abc&t=xyz", ""), ("t=xyz", ""), ("ref_src=twsrc%5Etfw", ""), ("ref_src=other", ""),
               ("igshid=abc", ""), ("igsh=abc", "https://l.instagram.com/")]:
    row(url=f"{URL}?{q}", referer=ref)

# --- Sec-Fetch, prefetch and client hints
for site in ["none", "same-origin", "same-site", "cross-site", "weird"]:
    for purpose in [None, "prefetch", "prefetch;prerender"]:
        h = {"sec_fetch_site": site}
        if purpose:
            h["sec_purpose"] = purpose
        row(**h)
        row(ua=REAL_UAS[7], **h)
for k in ["purpose", "x-purpose", "x-moz"]:
    row(**{k.replace("-", "_"): "prefetch"})
CH = [
    {"sec_ch_ua_platform": '"Windows"', "sec_ch_ua_platform_version": '"15.0.0"'},
    {"sec_ch_ua_platform": '"Windows"', "sec_ch_ua_platform_version": '"10.0.0"'},
    {"sec_ch_ua_platform": '"Windows"', "sec_ch_ua_platform_version": '"0.3.0"'},
    {"sec_ch_ua_platform": '"Android"', "sec_ch_ua_mobile": "?1", "sec_ch_ua_model": '"Pixel 9"'},
    {"sec_ch_ua_platform": '"macOS"', "sec_ch_ua_platform_version": '"15.1.0"'},
    {"sec_ch_ua_platform": '"Chrome OS"'},
    {"sec_ch_ua_platform": '"Linux"', "sec_ch_ua_form_factors": '"XR"'},
    {"sec_ch_ua_platform": '"Android"', "sec_ch_ua_form_factors": '"Tablet", "Desktop"'},
    {"sec_ch_ua_platform": '"Unknown"', "sec_ch_ua_mobile": "?0"},
    {"sec_ch_ua_model": '""'},
]
for h in CH:
    for ua in [CHROME_MAC, CHROME_WIN, CHROME_AND, REAL_UAS[20]]:
        row(ua=ua, **h)

# --- PWA, internal, relative URLs, self hosts
for q in ["utm_source=pwa", "source=homescreen", "src=a2hs", "utm_source=web_app_manifest"]:
    row(url=f"{URL}?{q}")
row(url="/p?gclid=1", self_hosts="shop.example")
row(url="/p", referer="https://shop.example/cart", self_hosts="shop.example")
row(url="/p", referer="https://blog.shop.example/", self_hosts="shop.example")
row(url="/p", referer="https://notshop.example/", self_hosts="shop.example")
row(url="/p", host="shop.example", referer="https://shop.example/")
row(url="https://SHOP.Example/p", referer="https://Shop.EXAMPLE/x")

# --- adversarial and malformed input: none of these may panic
ADVERSARIAL_URLS = [
    "", "/", "?", "not a url", "https://", "https://shop.example", "https://shop.example?",
    "https://shop.example/p?=", "https://shop.example/p?&&&", "https://shop.example/p?gclid",
    "https://shop.example/p?gclid=", "https://shop.example/p?utm_source=%ZZ&utm_medium=%E2%82%AC",
    "https://shop.example/p?utm_source=caf%C3%A9&utm_medium=social",
    "https://shop.example/p?utm_source=a+b&utm_medium=c%20d",
    "https://user:pass@shop.example:8443/p?gclid=1#frag",
    "https://shop.example/p#?gclid=1", "HTTPS://SHOP.EXAMPLE/P?GCLID=1",
    "https://xn--caf-dma.example/p", "https://shop.example/" + "a" * 3000 + "?gclid=1",
    "https://shop.example/p?" + "&".join(f"k{i}=v" for i in range(300)),
    "ftp://shop.example/p", "mailto:someone@example.com", "javascript:alert(1)",
    "https://[::1]/p?gclid=1", "https://127.0.0.1:8080/p?utm_source=google&utm_medium=cpc",
]
for u in ADVERSARIAL_URLS:
    row(url=u)
ADVERSARIAL_REFS = [
    "-", "null", "about:blank", "https://", "//t.co/", "t.co", "android-app://", "android-app:///",
    "https://www.google.com:443/search?q=x", "https://WWW.GOOGLE.COM/", "https://www.google.com./",
    "https://google.com@evil.example/", "https://evil.example/?ref=google.com",
    "https://xn--80ak6aa92e.com/", "https://例え.jp/", "https://www.google.com/search?q=%",
    "https://" + "a." * 120 + "example/", "https://chatgpt.com.evil.example/",
    "https://t.co.evil.example/", "https://evilt.co/",
]
for r in ADVERSARIAL_REFS:
    row(referer=r)
ADVERSARIAL_UAS = [
    "", "-", " ", "Mozilla/5.0", "Mozilla/5.0 (compatible)", "x" * 5000,
    "Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) " * 40,
    "Mozilla/5.0 (Linux; Android 14; ünïcødé Build/1; wv) Chrome/1 Mobile Safari/537.36 Instagram 1",
    "Mozilla/5.0 (Ｆｕｌｌｗｉｄｔｈ) Googlebot", "Googlebot\u00e9", "\u00e9Googlebot", "GOOGLEBOT/2.1",
    "Mozilla/5.0 " + "(" * 500, "Mozilla/5.0 ✓ 🚀 Twitter for iPhone/1", "curl/8.0", "python-requests/2.32",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) HeadlessChrome/141.0.0.0 Safari/537.36",
]
for u in ADVERSARIAL_UAS:
    row(ua=u)
for v in ["", " ", '"', '""', "?1", "?0", '"Windows"', "Windows", '"' * 50, "\u00e9"]:
    row(sec_ch_ua_platform=v, sec_ch_ua_mobile=v, sec_ch_ua_platform_version=v,
        sec_ch_ua_form_factors=v, sec_ch_ua_model=v, x_requested_with=v, sec_fetch_site=v)

# --- random combinations of everything above, to cover interactions
pool_q = [f"{c['param']}=1" for c in R["clickIds"]] + ["utm_source=facebook&utm_medium=paid_social",
          "utm_source=google&utm_medium=organic", "si=1", "s=20&t=1", "utm_medium=email"]
pool_r = ["", "https://t.co/", "https://www.google.com/search?q=x", "https://l.facebook.com/",
          "https://chatgpt.com/", "https://shop.example/", "android-app://com.google.android.gm/",
          "https://news.ycombinator.com/item?id=1", "https://mastodon.social/@a"]
pool_h = [{}, {"sec_fetch_site": "cross-site"}, {"sec_purpose": "prefetch"},
          {"x_requested_with": "com.instagram.android"}, {"sec_ch_ua_platform": '"Android"', "sec_ch_ua_mobile": "?1"}]
for _ in range(1500):
    qs = "&".join(rng.sample(pool_q, rng.randint(0, 3)))
    row(url=URL + ("?" + qs if qs else ""), referer=rng.choice(pool_r), ua=rng.choice(REAL_UAS),
        self_hosts=rng.choice(["", "shop.example"]), **rng.choice(pool_h))

# --- fetch-metadata grid: destination x initiator x mode/activation x client x referrer.
# Appended after the random rows so that adding it left every earlier row in place.
GRID_UAS = [CHROME_MAC, REAL_UAS[6], REAL_UAS[5], REAL_UAS[7],
            "Mozilla/5.0 (Linux; Android 5.1.1; Nexus 5 Build/LMY48B; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/43.0.2357.65 Mobile Safari/537.36"]
for dest in ["document", "iframe", "frame", "embed", "object", "image", "empty", "fencedframe"]:
    for site in ["cross-site", "same-site", "same-origin", "none", None]:
        for mode, user in [("navigate", "?1"), ("navigate", None), ("no-cors", None)]:
            for ua in GRID_UAS:
                for ref in ["", "https://t.co/", "https://shop.example/", "https://news.example/"]:
                    h = {"sec_fetch_dest": dest, "sec_fetch_mode": mode}
                    if site:
                        h["sec_fetch_site"] = site
                    if user:
                        h["sec_fetch_user"] = user
                    row(referer=ref, ua=ua, **h)

with open(os.path.join(HERE, "requests.tsv"), "w") as f:
    f.write("\n".join(rows) + "\n")

# --- combined-log lines for parse_combined_log
LOG = []
for i, ua in enumerate(REAL_UAS + ["-", "", 'quote " inside', "Googlebot/2.1 (+http://www.google.com/bot.html)"]):
    ref = ["-", "", "https://t.co/", "https://www.google.com/search?q=a%20b", 'https://a.example/"q"'][i % 5]
    path = ["/p", "/p?gclid=1", "/p?utm_source=x&utm_medium=social", "http://other.example/abs", "/"][i % 5]
    ua_f = ua.replace('"', '\\"')
    ref_f = ref.replace('"', '\\"')
    LOG.append(f'203.0.113.{i} - - [24/Oct/2026:10:00:{i % 60:02d} +0000] "GET {path} HTTP/1.1" 200 {i} "{ref_f}" "{ua_f}"')
LOG += [
    "", "garbage", '1.2.3.4 - - [x] "GET /p HTTP/1.1" 200 1 "-"',
    '1.2.3.4 - - [24/Oct/2026:10:00:00 +0000] "POST /api HTTP/2.0" 201 - "-" "curl/8.0"',
    '1.2.3.4 - - [24/Oct/2026:10:00:00 +0000] "GET /p HTTP/1.1" 304 0 "-" "-" "extra" "fields"',
    '::1 - frank [24/Oct/2026:10:00:00 -0500] "GET /p?fbclid=x HTTP/1.1" 200 5 "https://l.facebook.com/" "Mozilla/5.0"',
    '1.2.3.4 - - [24/Oct/2026:10:00:00 +0000] "get /p HTTP/1.1" 200 1 "-" "-"',
    '1.2.3.4 - - [24/Oct/2026:10:00:00 +0000] "GET  HTTP/1.1" 200 1 "-" "-"',
    '1.2.3.4 - - [24/Oct/2026:10:00:00 +0000] "GET /p HTTP/1.1" 2000 1 "-" "-"',
    '1.2.3.4 - - [24/Oct/2026:10:00:00 +0000] "GET /p" 200 1 "-" "Mozilla\\\\" ',
]
with open(os.path.join(HERE, "access.log"), "w") as f:
    f.write("\n".join(LOG) + "\n")

print(f"{len(rows)} requests, {len(LOG)} log lines")
