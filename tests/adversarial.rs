//! Adversarial cases: conflicting, stripped, nested, malformed and hostile requests. Each case
//! states the attribution it expects and why; the reasoning for the engine rules behind them is in
//! docs/CRITIQUE.md (pass 5).
//!
//! User agents are verbatim from the source cited above each constant; none is invented.
use std::collections::BTreeMap;
use traffic_sources::json::ToJson;
use traffic_sources::*;

// ------------------------------------------------------------------ user agents

// Chrome's reduced UA, the documented desktop and Android forms
// (chromium.org/updates/ua-reduction); the same strings as tests/golden/generate.py.
const CHROME_MAC: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";
const CHROME_AND: &str = "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Mobile Safari/537.36";
// tests/golden/generate.py REAL_UAS (Safari on iOS 18.6, Firefox on Linux).
const SAFARI_IOS: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.6 Mobile/15E148 Safari/604.1";
const FIREFOX: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:144.0) Gecko/20100101 Firefox/144.0";
// Android System WebView, the documented example (developer.chrome.com/docs/multidevice/user-agent).
const PLAIN_WV: &str = "Mozilla/5.0 (Linux; Android 5.1.1; Nexus 5 Build/LMY48B; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/43.0.2357.65 Mobile Safari/537.36";

// X in-app browser. user-agents.net sample (as tests/sourced.rs).
const X_AND: &str = "Mozilla/5.0 (Linux; Android 16; 23078PND5G Build/BP2A.250605.031.A3; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/153.0.8010.39 Mobile Safari/537.36 TwitterAndroid/12.32.0-prod.01 (312320001-r-01) 23078PND5G/16 (Xiaomi;23078PND5G;Xiaomi;corot_global;0;;1;2015)";
// uap-core / UAParser docs sample (as tests/sourced.rs).
const X_IOS: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_6_2 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/22G100 Twitter for iPhone/12.31.2";
// uap-core tests/test_ua.yaml (Apache-2.0).
const X_IPAD: &str = "Mozilla/5.0 (iPad; U; CPU OS 4_3_5 like Mac OS X; en-us) AppleWebKit/533.17.9 (KHTML, like Gecko) Mobile/8L1 Twitter for iPad";
// uap-core tests/test_ua.yaml (Apache-2.0): a bare `TwitterAndroid` suffix, no version.
const X_AND_BARE: &str = "Mozilla/5.0 (Linux; Android 9; Nokia 2.1 Build/PKQ1.181105.001; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/74.0.3729.157 Mobile Safari/537.36 TwitterAndroid";

// Other in-app browsers, uap-core tests/test_ua.yaml and test_device.yaml (Apache-2.0).
const FB_AND: &str = "Mozilla/5.0 (Linux; Android 5.1.1; R7sf Build/LMY47V; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/80.0.3987.149 Mobile Safari/537.36 [FB_IAB/FB4A;FBAV/263.0.0.46.121;]";
const IG_IOS: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 11_2_5 like Mac OS X) AppleWebKit/604.5.6 (KHTML, like Gecko) Mobile/15D60 Instagram 33.0.0.11.96 (iPhone9,3; iOS 11_2_5; en_AU; en-AU; scale=2.00; gamut=wide; 750x1334)";
const TIKTOK_IOS: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_3_1 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.3.1 Mobile/15E148 Safari/604.1 musical_ly_34.3.1 JsSdk/2.0 NetType/4G Channel/App Store ByteLocale/en Region/US isDarkMode/1 WKWebView/1 RevealType/Dialog";
const KAKAO_IOS: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148 Safari/604.1 KAKAOTALK/11.3.1 (INAPP)";
const LINE_AND: &str = "Mozilla/5.0 (Linux; Android 5.1; FTJ152B Build/LMY47D; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/51.0.2704.81 Mobile Safari/537.36 Line/6.4.1";
const GSA_AND: &str = "Mozilla/5.0 (Linux; Android 8.0.0; SM-G960F Build/R16NW; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/71.0.3578.99 Mobile Safari/537.36 GSA/8.65.5.21.arm64";
const SNAP_IOS: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 11_2_6 like Mac OS X) AppleWebKit/604.5.6 (KHTML, like Gecko) Snapchat/10.38.0.25 (iPhone8,1; iOS 11.2.6; gzip)";
const TELEGRAM_AND: &str = "Mozilla/5.0 (Linux; Android 15; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/144.0.7559.132 Safari/537.36 Telegram-Android/12.4.1 (Xiaomi 2405CRPFDG; Android 15; SDK 35; HIGH)";

// Mail clients: uap-core tests/test_ua.yaml (Outlook); tests/golden/generate.py (Thunderbird).
const OUTLOOK: &str = "Microsoft Office/14.0 (Windows NT 6.1; Microsoft Outlook 14.0.5128; Pro)";
const THUNDERBIRD: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:128.0) Gecko/20100101 Thunderbird/128.3.1";

// Consoles, TV, XR, car. uap-core (Quest 3, Cast on Fuchsia, webOS, Roku); tests/golden/generate.py
// (PlayStation 5, Fire TV, Tesla, Xbox).
const QUEST3: &str = "Mozilla/5.0 (X11; Linux x86_64; Quest 3) AppleWebKit/537.36 (KHTML, like Gecko) OculusBrowser/26.2.0.0.10 Chrome/110.0.5481.192 VR Safari/537.36";
const CHROMECAST: &str = "Mozilla/5.0 (Fuchsia) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 CrKey/1.56.500000";
const WEBOS: &str = "Mozilla/5.0 (Web0S; Linux/SmartTV) AppleWebKit/537.41 (KHTML, like Gecko) Large Screen WebAppManager Safari/537.41";
const ROKU: &str = "Roku/DVP-5.0 (025.00E08043A)";
const PS5: &str = "Mozilla/5.0 (PlayStation 5 3.11) AppleWebKit/605.1.15 (KHTML, like Gecko)";
const FIRETV: &str = "Mozilla/5.0 (Linux; Android 9; AFTMM) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/89.0.4389.90 Mobile Safari/537.36";
const TESLA: &str =
    "Mozilla/5.0 (X11; Linux) AppleWebKit/534.34 (KHTML, like Gecko) QtCarBrowser Safari/534.34";
const XBOX: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; Xbox; Xbox One) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/70.0.3538.102 Safari/537.36 Edge/18.19041";

// Link unfurlers and fetchers, verbatim `instances` from
// github.com/monperrus/crawler-user-agents crawler-user-agents.json (MIT).
const TWITTERBOT: &str = "Twitterbot/1.0";
const UNFURLERS: &[(&str, &str)] = &[
    ("Twitterbot/1.0", "x"),
    ("Twitterbot/0.1", "x"),
    ("Slackbot-LinkExpanding 1.0 (+https://api.slack.com/robots)", "slack"),
    ("Mozilla/5.0 (compatible; Discordbot/2.0; +https://discordapp.com)", "discord"),
    ("facebookexternalhit/1.1", "facebook"),
    ("facebookexternalhit/1.0 (+http://www.facebook.com/externalhit_uatext.php)", "facebook"),
    ("WhatsApp/0.3.4479 N", "whatsapp"),
    ("WhatsApp", "whatsapp"),
    ("TelegramBot (like TwitterBot)", "telegram"),
    ("LinkedInBot/1.0 (compatible; Mozilla/5.0; Jakarta Commons-HttpClient/3.1 +http://www.linkedin.com)", "linkedin"),
    ("Mozilla/5.0 (compatible; redditbot/1.0; +http://www.reddit.com/feedback)", "reddit"),
    ("Mozilla/5.0 (compatible; Embedly/0.2; +http://support.embed.ly/)", "embedly"),
    ("Iframely/1.3.1 (+https://iframely.com/docs/about) Atlassian", "iframely"),
    ("Mozilla/5.0 (Windows NT 6.1; WOW64) SkypeUriPreview Preview/0.5", "skype"),
    ("Mozilla/5.0 (compatible; vkShare; +http://vk.com/dev/Share)", "vk"),
    ("BlueskyPreviewBot/1.0", "bluesky"),
    ("http.rb/2.2.2 (Mastodon/1.5.1; +https://example-masto-instance.org/)", "mastodon"),
];
const GPTBOT: &str = "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; GPTBot/1.0; +https://openai.com/gptbot)";
const CHATGPT_USER: &str = "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko); compatible; ChatGPT-User/1.0; +https://openai.com/bot";
const CLAUDE_USER: &str = "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; Claude-User/1.0; +Claude-User@anthropic.com)";
const CLAUDE_CODE: &str = "Claude-User (claude-code/2.1.86; +https://support.anthropic.com/)";
const PERPLEXITY_USER: &str = "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; Perplexity-User/1.0; +https://perplexity.ai/perplexity-user)";
const PERPLEXITYBOT: &str = "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; PerplexityBot/1.0; +https://perplexity.ai/perplexitybot)";
const OAI_SEARCH: &str = "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko); compatible; OAI-SearchBot/1.0; +https://openai.com/searchbot";
const INSPECTION_TOOL: &str = "Mozilla/5.0 (compatible; Google-InspectionTool/1.0)";
const SNAPCHAT_ADS: &str = "Mozilla/5.0 (compatible; SnapchatAds/1.0; +https://businesshelp.snapchat.com/s/article/adsbot-crawler)";

// ------------------------------------------------------------------ helpers

const URL: &str = "https://shop.example/p";

/// (url, referer, headers)
type Variant<'a> = (&'a str, Option<&'a str>, &'a [(&'a str, &'a str)]);

fn eng() -> &'static Attributor {
    static E: std::sync::OnceLock<Attributor> = std::sync::OnceLock::new();
    E.get_or_init(Attributor::new)
}
fn req(url: &str, referer: Option<&str>, ua: &str, headers: &[(&str, &str)]) -> Request {
    Request {
        url: url.into(),
        referer: referer.map(String::from),
        user_agent: Some(ua.into()),
        headers: headers
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect::<BTreeMap<_, _>>(),
        ..Default::default()
    }
}
fn at(url: &str, referer: Option<&str>, ua: &str, headers: &[(&str, &str)]) -> Attribution {
    eng().attribute(&req(url, referer, ua, headers))
}
fn has(a: &Attribution, ch: Channel, src: &str) -> bool {
    a.candidates
        .iter()
        .any(|c| c.channel == ch && c.source == src)
}
fn summary(a: &Attribution) -> String {
    a.candidates
        .iter()
        .map(|c| {
            format!(
                "{}/{} {:.3} {}",
                c.channel, c.source, c.confidence, c.interaction
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}
#[track_caller]
fn top(a: &Attribution, ch: Channel, src: &str) {
    assert!(
        a.top().channel == ch && a.top().source == src,
        "expected top {ch}/{src}, got {}",
        summary(a)
    );
}
#[track_caller]
fn no_direct(a: &Attribution) {
    assert!(
        !has(a, Channel::Direct, "(direct)"),
        "direct offered: {}",
        summary(a)
    );
}

// ------------------------------------------------------------------ frames, no referrer

const EMBED_DESTS: [&str; 5] = ["iframe", "frame", "embed", "object", "fencedframe"];
/// (Sec-Fetch-Mode, Sec-Fetch-User): a click inside a frame navigates it with user activation;
/// a plugin `<embed>` fetches it no-cors. Neither changes who embeds the page.
const MODES: [(&str, Option<&str>); 3] = [
    ("navigate", Some("?1")),
    ("navigate", None),
    ("no-cors", None),
];

fn fetch(
    dest: &str,
    site: Option<&str>,
    mode: &str,
    user: Option<&str>,
) -> Vec<(&'static str, String)> {
    let mut h = vec![
        ("Sec-Fetch-Dest", dest.to_string()),
        ("Sec-Fetch-Mode", mode.to_string()),
    ];
    if let Some(s) = site {
        h.push(("Sec-Fetch-Site", s.to_string()));
    }
    if let Some(u) = user {
        h.push(("Sec-Fetch-User", u.to_string()));
    }
    h
}
fn at_h(url: &str, referer: Option<&str>, ua: &str, h: &[(&'static str, String)]) -> Attribution {
    let h: Vec<(&str, &str)> = h.iter().map(|(k, v)| (*k, v.as_str())).collect();
    at(url, referer, ua, &h)
}

/// A cross-site frame with the referrer stripped is some other site embedding this page. It is
/// not a click, so neither direct nor dark social, and it does not start a visit.
#[test]
fn cross_site_frame_without_referrer_is_an_embedding_referral() {
    for dest in EMBED_DESTS {
        for (mode, user) in MODES {
            let a = at_h(
                URL,
                None,
                CHROME_MAC,
                &fetch(dest, Some("cross-site"), mode, user),
            );
            top(&a, Channel::Referral, "(embedding page)");
            assert_eq!(a.top().interaction, Interaction::Embed, "{dest} {mode}");
            assert!(!a.new_touch, "{dest} {mode} {user:?}");
            no_direct(&a);
            assert!(
                !a.candidates
                    .iter()
                    .any(|c| c.channel == Channel::DarkSocial),
                "{}",
                summary(&a)
            );
            assert!((a.top().confidence - weights::EMBED_NO_REFERRER).abs() < 1e-9);
        }
    }
}

/// Same-site or same-origin frames are the site framing itself (a widget, a checkout iframe).
#[test]
fn same_site_frame_without_referrer_is_internal() {
    for dest in EMBED_DESTS {
        for site in ["same-site", "same-origin"] {
            for (mode, user) in MODES {
                let a = at_h(URL, None, CHROME_MAC, &fetch(dest, Some(site), mode, user));
                top(&a, Channel::Internal, "(self)");
                assert_eq!(a.top().interaction, Interaction::Embed);
                assert!(!a.new_touch);
                no_direct(&a);
            }
        }
    }
}

/// `none` is not possible for a nested frame per Fetch, and a missing Sec-Fetch-Site means an
/// older client; either way the frame still has an embedder, just a weaker one.
#[test]
fn frame_with_unknown_initiator_is_a_weak_embedding_referral() {
    for dest in EMBED_DESTS {
        for site in [Some("none"), None, Some("garbage")] {
            let a = at_h(URL, None, CHROME_MAC, &fetch(dest, site, "navigate", None));
            top(&a, Channel::Referral, "(embedding page)");
            assert!((a.top().confidence - 0.3).abs() < 1e-9, "{}", summary(&a));
            assert!(!a.new_touch);
        }
    }
}

#[test]
fn fetch_metadata_values_are_case_and_whitespace_insensitive() {
    let a = at(
        URL,
        None,
        CHROME_MAC,
        &[
            ("sec-fetch-dest", " IFRAME "),
            ("SEC-FETCH-SITE", "Cross-Site"),
        ],
    );
    top(&a, Channel::Referral, "(embedding page)");
    assert_eq!(a.top().interaction, Interaction::Embed);
}

/// With a referrer the frame is attributed to it as usual; it is still an embed, not a visit.
#[test]
fn frame_with_referrer_keeps_the_referrer_but_is_not_a_touch() {
    let a = at(
        URL,
        Some("https://news.example/story"),
        CHROME_MAC,
        &[
            ("Sec-Fetch-Dest", "iframe"),
            ("Sec-Fetch-Site", "cross-site"),
        ],
    );
    top(&a, Channel::Referral, "news.example");
    assert_eq!(a.top().interaction, Interaction::Embed);
    assert!(!a.new_touch);
    assert!(a
        .evidence
        .iter()
        .any(|e| e.signal == "sec-fetch-dest:iframe" && e.weight == 0.0));
}

/// Controls: a top-level document keeps the established behaviour.
#[test]
fn top_level_document_without_referrer_is_unchanged() {
    let cross = at(
        URL,
        None,
        CHROME_MAC,
        &[
            ("Sec-Fetch-Dest", "document"),
            ("Sec-Fetch-Site", "cross-site"),
            ("Sec-Fetch-Mode", "navigate"),
        ],
    );
    top(&cross, Channel::DarkSocial, "(referrer stripped)");
    assert!(cross.new_touch);
    let typed = at(
        URL,
        None,
        CHROME_MAC,
        &[
            ("Sec-Fetch-Dest", "document"),
            ("Sec-Fetch-Site", "none"),
            ("Sec-Fetch-User", "?1"),
        ],
    );
    top(&typed, Channel::Direct, "(direct)");
    assert!(typed.new_touch);
    let same = at(
        URL,
        None,
        CHROME_MAC,
        &[
            ("Sec-Fetch-Dest", "document"),
            ("Sec-Fetch-Site", "same-origin"),
        ],
    );
    top(&same, Channel::Internal, "(self)");
    assert!(!same.new_touch);
    assert_eq!(same.candidates.len(), 1, "{}", summary(&same));
}

/// Subresources (pixels, scripts, fetch()) are not page views at all.
#[test]
fn subresource_requests_are_never_touches() {
    for dest in [
        "image", "script", "style", "empty", "font", "video", "worker", "json", "manifest",
    ] {
        for referer in [None, Some("https://www.google.com/")] {
            let a = at(
                URL,
                referer,
                CHROME_MAC,
                &[("Sec-Fetch-Dest", dest), ("Sec-Fetch-Site", "cross-site")],
            );
            assert_eq!(a.top().interaction, Interaction::Subresource, "{dest}");
            assert!(!a.new_touch, "{dest}");
        }
    }
    // the referrer is kept: a hotlinked image on google.com is still google's
    let a = at(
        URL,
        Some("https://www.google.com/"),
        CHROME_MAC,
        &[("Sec-Fetch-Dest", "image")],
    );
    assert_eq!(a.top().source, "google");
}

// ------------------------------------------------------------------ X (Twitter), no referrer

/// The UA names X's in-app browser, so the click happened inside X: organic social, not direct.
/// X strips the referrer from its in-app browser, which is why this case exists at all.
#[test]
fn x_in_app_browser_without_referrer_is_organic_social_x() {
    for ua in [X_IOS, X_IPAD, X_AND, X_AND_BARE] {
        for h in [
            vec![],
            vec![("Sec-Fetch-Site", "none")],
            vec![
                ("Sec-Fetch-Site", "cross-site"),
                ("Sec-Fetch-Dest", "document"),
            ],
        ] {
            let a = at(URL, None, ua, &h);
            top(&a, Channel::OrganicSocial, "x");
            assert!(
                (a.top().confidence - weights::UA_TOKEN).abs() < 1e-9,
                "{}",
                summary(&a)
            );
            assert_eq!(a.top().interaction, Interaction::Click);
            assert!(a.new_touch);
            assert_eq!(a.client.surface, Surface::WebviewIab);
            assert_eq!(a.client.app.as_deref(), Some("x"));
            no_direct(&a);
        }
    }
}

/// Android's WebView reports the host app in X-Requested-With; that names X even when the UA
/// is a plain WebView.
#[test]
fn x_requested_with_twitter_is_organic_social_x() {
    for ua in [PLAIN_WV, CHROME_AND] {
        let a = at(
            URL,
            None,
            ua,
            &[("X-Requested-With", "com.twitter.android")],
        );
        top(&a, Channel::OrganicSocial, "x");
        assert_eq!(a.client.app.as_deref(), Some("x"));
        assert!(a.new_touch);
    }
    // header-name case does not matter
    let a = at(
        URL,
        None,
        PLAIN_WV,
        &[("x-requested-with", "com.twitter.android")],
    );
    top(&a, Channel::OrganicSocial, "x");
}

/// A plain WebView without X-Requested-With names no app: direct stays the best guess, with dark
/// social (an unknown app stripped the referrer) as the alternative.
#[test]
fn anonymous_webview_is_direct_with_a_dark_social_alternative() {
    let a = at(URL, None, PLAIN_WV, &[]);
    top(&a, Channel::Direct, "(direct)");
    assert!(
        has(&a, Channel::DarkSocial, "(in-app, unidentified)"),
        "{}",
        summary(&a)
    );
}

/// Inside X's browser, a same-origin navigation is the user moving around this site. The UA
/// says which app the browser lives in, not where this navigation came from.
#[test]
fn x_in_app_same_origin_navigation_is_internal() {
    for site in ["same-origin", "same-site"] {
        let a = at(URL, None, X_IOS, &[("Sec-Fetch-Site", site)]);
        top(&a, Channel::Internal, "(self)");
        assert!(!a.new_touch);
        assert!(!has(&a, Channel::OrganicSocial, "x"), "{}", summary(&a));
    }
    let a = at(
        URL,
        None,
        PLAIN_WV,
        &[
            ("Sec-Fetch-Site", "same-origin"),
            ("X-Requested-With", "com.twitter.android"),
        ],
    );
    top(&a, Channel::Internal, "(self)");
}

/// X's browser showing a page that frames this one: the frame is X-originated but not a visit.
#[test]
fn x_in_app_frame_is_x_but_not_a_touch() {
    for dest in EMBED_DESTS {
        let a = at(
            URL,
            None,
            X_AND,
            &[("Sec-Fetch-Dest", dest), ("Sec-Fetch-Site", "cross-site")],
        );
        top(&a, Channel::OrganicSocial, "x");
        assert_eq!(a.top().interaction, Interaction::Embed);
        assert!(
            has(&a, Channel::Referral, "(embedding page)"),
            "{}",
            summary(&a)
        );
        assert!(!a.new_touch);
        no_direct(&a);
    }
}

#[test]
fn x_in_app_with_t_co_referrer_agrees_and_strengthens() {
    let a = at(URL, Some("https://t.co/abc123"), X_IOS, &[]);
    top(&a, Channel::OrganicSocial, "x");
    assert!(a.top().confidence > weights::UA_TOKEN, "{}", summary(&a));
    assert_eq!(a.candidates.len(), 1);
}

/// Explicit, complete tagging wins over the app: the link was built for a Facebook campaign
/// and someone opened it inside X, so Facebook is first and X is the runner-up.
#[test]
fn x_in_app_with_full_facebook_utm_ranks_the_tag_first() {
    let a = at(
        &format!("{URL}?utm_source=facebook&utm_medium=social"),
        None,
        X_IOS,
        &[],
    );
    top(&a, Channel::OrganicSocial, "facebook");
    assert!(has(&a, Channel::OrganicSocial, "x"));
}

/// When t.co confirms the app, two independent signals say this click happened on X; the tag
/// describes where the link was first published (a copied ad link), so it is the runner-up.
#[test]
fn x_in_app_via_t_co_outranks_a_copied_campaign_link() {
    let a = at(
        &format!("{URL}?utm_source=facebook&utm_medium=paid_social"),
        Some("https://t.co/"),
        X_AND,
        &[],
    );
    top(&a, Channel::OrganicSocial, "x");
    assert!(has(&a, Channel::PaidSocial, "facebook"), "{}", summary(&a));
    assert!(a.candidates[1].confidence >= weights::UTM_FULL - 1e-9);
}

/// A bare `utm_source` is weaker than the UA: X is first, the tag second.
#[test]
fn x_in_app_with_partial_facebook_utm_ranks_the_app_first() {
    let a = at(&format!("{URL}?utm_source=facebook"), None, X_IOS, &[]);
    top(&a, Channel::OrganicSocial, "x");
    assert!(
        has(&a, Channel::OrganicSocial, "facebook"),
        "{}",
        summary(&a)
    );
}

/// fbclid is appended by Meta's link shim to any outbound link; it says the link once passed
/// through Facebook, not that this click happened there. The app in hand is X.
#[test]
fn x_in_app_with_fbclid_is_still_x() {
    let a = at(&format!("{URL}?fbclid=IwAR0abc"), None, X_IOS, &[]);
    top(&a, Channel::OrganicSocial, "x");
}

#[test]
fn x_in_app_with_twclid_is_paid_x() {
    let a = at(&format!("{URL}?twclid=2-abc"), None, X_IOS, &[]);
    top(&a, Channel::PaidSocial, "x");
    assert_eq!(a.top().interaction, Interaction::AdClick);
}

/// The referrer is Facebook's redirector; the click happened in X's browser.
#[test]
fn x_in_app_through_the_facebook_link_shim_is_x_first() {
    let a = at(URL, Some("https://l.facebook.com/l.php?u=x"), X_IOS, &[]);
    top(&a, Channel::OrganicSocial, "x");
    assert!(has(&a, Channel::OrganicSocial, "facebook"));
}

#[test]
fn x_in_app_prefetch_keeps_x_but_is_not_a_touch() {
    let a = at(URL, None, X_IOS, &[("Sec-Purpose", "prefetch")]);
    top(&a, Channel::OrganicSocial, "x");
    assert_eq!(a.top().interaction, Interaction::Prefetch);
    assert!(a.is_prefetch && !a.new_touch);
}

// ------------------------------------------------------------------ unfurlers

/// A link-preview fetch is a bot, whatever else the request carries: no human saw the page.
#[test]
fn unfurlers_are_excluded_bots_whatever_else_the_request_says() {
    let extras: [Variant; 5] = [
        (URL, None, &[]),
        (
            URL,
            None,
            &[
                ("Sec-Fetch-Dest", "iframe"),
                ("Sec-Fetch-Site", "cross-site"),
            ],
        ),
        (URL, Some("https://t.co/abc"), &[]),
        (
            "https://shop.example/p?utm_source=facebook&utm_medium=social&fbclid=1",
            None,
            &[],
        ),
        (URL, None, &[("X-Requested-With", "com.twitter.android")]),
    ];
    for (ua, op) in UNFURLERS {
        for (url, referer, h) in extras {
            let a = at(url, referer, ua, h);
            let b = a
                .bot
                .as_ref()
                .unwrap_or_else(|| panic!("{ua}: not a bot: {}", summary(&a)));
            assert_eq!(b.class, BotClass::LinkPreview, "{ua}");
            assert_eq!(b.operator, *op, "{ua}");
            assert!(b.exclude, "{ua}");
            assert_eq!(a.top().channel, Channel::Bot, "{ua}");
            assert_eq!(a.top().interaction, Interaction::PreviewUnfurl, "{ua}");
            assert_eq!(a.candidates.len(), 1, "{ua}: {}", summary(&a));
            assert_eq!(a.client.surface, Surface::Bot, "{ua}");
            assert_eq!(a.client.app, None, "{ua}");
            assert!(!a.new_touch, "{ua}");
        }
    }
}

#[test]
fn twitterbot_borrowing_x_in_app_markers_is_still_a_bot() {
    let a = at(
        URL,
        Some("https://t.co/"),
        TWITTERBOT,
        &[
            ("X-Requested-With", "com.twitter.android"),
            ("Sec-Fetch-Dest", "document"),
        ],
    );
    assert_eq!(a.bot.as_ref().unwrap().operator, "x");
    assert_eq!(a.client.surface, Surface::Bot);
}

// ------------------------------------------------------------------ AI assistants and fetchers

/// A user-triggered assistant fetch is a person's request by proxy: excluded from human traffic
/// but kept as an ai_assistant touch.
#[test]
fn ai_user_fetchers_are_proxied_humans() {
    for (ua, op) in [
        (CHATGPT_USER, "chatgpt"),
        (CLAUDE_USER, "claude"),
        (CLAUDE_CODE, "claude"),
        (PERPLEXITY_USER, "perplexity"),
    ] {
        let a = at(URL, None, ua, &[]);
        let b = a.bot.as_ref().unwrap();
        assert_eq!(
            (b.class, b.operator.as_str()),
            (BotClass::AiUserFetch, op),
            "{ua}"
        );
        assert!(!b.exclude);
        assert!(has(&a, Channel::AiAssistant, op), "{}", summary(&a));
        assert!(!a.new_touch);
    }
}

/// Tokens appearing in a trailing URL must not outrank the product token.
#[test]
fn bot_operator_comes_from_the_product_token_not_the_url() {
    let b = eng().classify_bot(&req(URL, None, GPTBOT, &[])).unwrap();
    assert_eq!(
        (b.token.as_str(), b.operator.as_str()),
        ("GPTBot", "openai")
    );
    let b = eng()
        .classify_bot(&req(URL, None, CLAUDE_CODE, &[]))
        .unwrap();
    assert_eq!(b.token, "Claude-User");
    for (ua, class, op) in [
        (PERPLEXITYBOT, BotClass::AiSearchCrawler, "perplexity"),
        (OAI_SEARCH, BotClass::AiSearchCrawler, "openai"),
        (INSPECTION_TOOL, BotClass::SeoTool, "google"),
        (SNAPCHAT_ADS, BotClass::AdVerifier, "snapchat"),
    ] {
        let b = eng().classify_bot(&req(URL, None, ua, &[])).unwrap();
        assert_eq!((b.class, b.operator.as_str()), (class, op), "{ua}");
    }
}

#[test]
fn ai_assistant_referrers_and_tags() {
    for (r, src) in [
        ("https://chatgpt.com/", "chatgpt"),
        ("https://www.perplexity.ai/search?q=x", "perplexity"),
        ("https://gemini.google.com/app", "gemini"),
        ("https://copilot.microsoft.com/", "copilot"),
        ("https://claude.ai/chat/1", "claude"),
        ("https://x.com/i/grok", "grok"),
    ] {
        let a = at(URL, Some(r), CHROME_MAC, &[]);
        top(&a, Channel::AiAssistant, src);
    }
    top(
        &at(
            &format!("{URL}?utm_source=chatgpt.com"),
            None,
            SAFARI_IOS,
            &[],
        ),
        Channel::AiAssistant,
        "chatgpt",
    );
    // x.com outside /i/grok is X itself
    top(
        &at(URL, Some("https://x.com/someone/status/1"), CHROME_MAC, &[]),
        Channel::OrganicSocial,
        "x",
    );
}

// ------------------------------------------------------------------ conflicting signals

#[test]
fn conflicting_tags_rank_by_strength() {
    // full UTM beats a click ID from another network; the click ID stays as an alternative
    let a = at(
        &format!("{URL}?utm_source=google&utm_medium=cpc&fbclid=1"),
        None,
        CHROME_MAC,
        &[],
    );
    top(&a, Channel::PaidSearch, "google");
    assert!(has(&a, Channel::OrganicSocial, "facebook"));
    // two strong click IDs: both kept
    let a = at(&format!("{URL}?gclid=1&msclkid=2"), None, CHROME_MAC, &[]);
    assert!(has(&a, Channel::PaidSearch, "google") && has(&a, Channel::PaidSearch, "bing"));
    // gclid on a bing referrer: the ad click is google's, bing is the organic alternative
    let a = at(
        &format!("{URL}?gclid=1"),
        Some("https://www.bing.com/"),
        CHROME_MAC,
        &[],
    );
    top(&a, Channel::PaidSearch, "google");
    assert!(has(&a, Channel::OrganicSearch, "bing"));
    // a click ID beats the in-app UA of another network
    let a = at(&format!("{URL}?ttclid=1"), None, IG_IOS, &[]);
    top(&a, Channel::PaidSocial, "tiktok");
    assert!(has(&a, Channel::OrganicSocial, "instagram"));
}

/// GA4 takes the session source from utm_source; a referrer only fills a missing one.
#[test]
fn utm_source_decides_the_channel_not_the_referrer() {
    let a = at(
        &format!("{URL}?utm_source=newsletter&utm_medium=email"),
        Some("https://t.co/"),
        CHROME_MAC,
        &[],
    );
    top(&a, Channel::Email, "newsletter");
    let a = at(
        &format!("{URL}?utm_source=facebook&gclid=1"),
        Some("https://www.google.com/"),
        CHROME_MAC,
        &[],
    );
    assert!(
        has(&a, Channel::OrganicSocial, "facebook"),
        "{}",
        summary(&a)
    );
    assert!(
        !has(&a, Channel::OrganicSearch, "facebook"),
        "{}",
        summary(&a)
    );
    let a = at(
        &format!("{URL}?utm_source=facebook&utm_medium=paid_social"),
        Some("https://www.google.com/search?q=x"),
        SAFARI_IOS,
        &[],
    );
    top(&a, Channel::PaidSocial, "facebook");
}

/// With only utm_medium, the source is filled from the referrer under its canonical name, and
/// that name is what the channel is derived from.
#[test]
fn utm_medium_alone_takes_its_source_from_the_referrer() {
    let a = at(
        &format!("{URL}?utm_medium=email"),
        Some("android-app://com.google.android.gm/"),
        CHROME_AND,
        &[],
    );
    top(&a, Channel::Email, "gmail");
    assert_eq!(a.candidates.len(), 1, "{}", summary(&a));
    let a = at(
        &format!("{URL}?utm_medium=email"),
        Some("https://mastodon.social/@a"),
        CHROME_MAC,
        &[],
    );
    assert_eq!(a.top().source, "mastodon");
    let a = at(
        &format!("{URL}?utm_medium=cpc"),
        Some("https://www.google.com/"),
        CHROME_MAC,
        &[],
    );
    top(&a, Channel::PaidSearch, "google");
    let a = at(
        &format!("{URL}?utm_medium=email&fbclid=1"),
        Some("https://l.facebook.com/"),
        OUTLOOK,
        &[],
    );
    assert_eq!(a.top().source, "facebook", "{}", summary(&a));
    let a = at(&format!("{URL}?utm_medium=cpc"), None, CHROME_MAC, &[]);
    top(&a, Channel::PaidOther, "(not set)");
}

// ------------------------------------------------------------------ wrapper chains

/// Only the last hop of a redirect chain reaches the Referer; earlier hops are invisible.
#[test]
fn wrapper_chains_attribute_the_last_visible_hop() {
    for (r, src) in [
        (
            "https://l.facebook.com/l.php?u=https%3A%2F%2Ft.co%2Fx",
            "facebook",
        ),
        ("https://lm.facebook.com/", "facebook"),
        ("https://t.co/abc", "x"),
        ("https://lnkd.in/abc", "linkedin"),
        ("https://l.instagram.com/?u=x", "instagram"),
        ("https://out.reddit.com/t3_x", "reddit"),
    ] {
        top(
            &at(URL, Some(r), CHROME_MAC, &[]),
            Channel::OrganicSocial,
            src,
        );
    }
    // l.facebook -> t.co -> lnkd.in: LinkedIn's shim is the last hop
    let a = at(URL, Some("https://lnkd.in/abc"), FB_AND, &[]);
    top(&a, Channel::OrganicSocial, "facebook");
    assert!(has(&a, Channel::OrganicSocial, "linkedin"));
    // a tag set by the first hop survives the chain and wins
    let a = at(
        &format!("{URL}?utm_source=facebook&utm_medium=social"),
        Some("https://lnkd.in/abc"),
        CHROME_MAC,
        &[],
    );
    top(&a, Channel::OrganicSocial, "facebook");
}

// ------------------------------------------------------------------ self-referrals

#[test]
fn self_referrals_and_lookalikes() {
    for r in [
        "https://shop.example/cart",
        "https://SHOP.EXAMPLE/",
        "https://shop.example:8443/",
        "https://shop.example./",
        "https://blog.shop.example/x",
        "https://www.shop.example/x",
    ] {
        let a = at(URL, Some(r), CHROME_MAC, &[]);
        top(&a, Channel::Internal, "(self)");
        assert!(!a.new_touch, "{r}");
    }
    // www and apex are one site in both directions
    let a = at(
        "https://www.shop.example/p",
        Some("https://shop.example/"),
        CHROME_MAC,
        &[],
    );
    top(&a, Channel::Internal, "(self)");
    for r in [
        "https://evilshop.example/",
        "https://shop.example.evil.test/",
        "https://xshop.example/",
    ] {
        let a = at(URL, Some(r), CHROME_MAC, &[]);
        assert_eq!(a.top().channel, Channel::Referral, "{r}: {}", summary(&a));
        assert!(a.new_touch);
    }
    // explicit self_hosts cover a separate checkout domain
    let mut r = req(
        URL,
        Some("https://pay.shop-checkout.example/"),
        CHROME_MAC,
        &[],
    );
    r.self_hosts = vec!["shop.example".into(), "Shop-Checkout.Example.".into()];
    top(&eng().attribute(&r), Channel::Internal, "(self)");
}

/// An internal referrer means a link on this site; click IDs on it were copied along, so they are
/// ignored, and the visit is not a new touch even if a UTM is present (internal UTMs are a
/// tagging mistake, kept visible as a candidate).
#[test]
fn self_referral_suppresses_click_ids_and_new_touch() {
    let a = at(
        &format!("{URL}?gclid=1"),
        Some("https://shop.example/"),
        CHROME_MAC,
        &[],
    );
    top(&a, Channel::Internal, "(self)");
    let a = at(
        &format!("{URL}?utm_source=newsletter&utm_medium=email"),
        Some("https://shop.example/"),
        CHROME_MAC,
        &[],
    );
    assert!(has(&a, Channel::Internal, "(self)"));
    assert!(!a.new_touch);
    let a = at(URL, Some("https://shop.example/"), X_IOS, &[]);
    top(&a, Channel::Internal, "(self)");
}

// ------------------------------------------------------------------ prefetch / prerender

#[test]
fn speculative_loads_keep_their_source_but_are_not_touches() {
    for h in [
        ("Sec-Purpose", "prefetch"),
        ("Sec-Purpose", "prefetch;prerender"),
        ("Sec-Purpose", "prefetch;anonymous-client-ip"),
        ("Purpose", "prefetch"),
        ("X-Purpose", "preview"),
        ("X-Moz", "prefetch"),
    ] {
        let a = at(URL, Some("https://www.google.com/"), CHROME_AND, &[h]);
        if h.1 == "preview" {
            assert!(!a.is_prefetch, "{h:?}");
            continue;
        }
        assert!(a.is_prefetch, "{h:?}");
        assert!(!a.new_touch);
        assert_eq!(a.top().source, "google");
        assert_eq!(a.top().interaction, Interaction::Prefetch);
    }
    // a prefetched cross-site navigation without referrer is still dark social, just not a visit
    let a = at(
        URL,
        None,
        CHROME_MAC,
        &[
            ("Sec-Purpose", "prefetch"),
            ("Sec-Fetch-Site", "cross-site"),
        ],
    );
    top(&a, Channel::DarkSocial, "(referrer stripped)");
    assert!(!a.new_touch);
}

// ------------------------------------------------------------------ client hints

/// Only Chromium sends UA client hints, so hints that contradict a WebKit or Gecko UA were not
/// written by that browser and are ignored.
#[test]
fn client_hints_are_ignored_beside_non_chromium_user_agents() {
    let h = [
        ("Sec-CH-UA-Platform", "\"Windows\""),
        ("Sec-CH-UA-Mobile", "?0"),
    ];
    let c = eng().classify_client(&req(URL, None, SAFARI_IOS, &h));
    assert_eq!(
        (c.os, c.form_factor, c.client_hints_used),
        (Os::Ios, FormFactor::Phone, false)
    );
    let c = eng().classify_client(&req(
        URL,
        None,
        FIREFOX,
        &[
            ("Sec-CH-UA-Platform", "\"Android\""),
            ("Sec-CH-UA-Mobile", "?1"),
        ],
    ));
    assert_eq!(
        (c.os, c.form_factor, c.client_hints_used),
        (Os::Linux, FormFactor::Desktop, false)
    );
    // Chromium hints are trusted over the reduced UA
    let c = eng().classify_client(&req(
        URL,
        None,
        CHROME_MAC,
        &[
            ("Sec-CH-UA-Platform", "\"Windows\""),
            ("Sec-CH-UA-Platform-Version", "\"15.0.0\""),
        ],
    ));
    assert_eq!(c.os, Os::Windows);
    assert_eq!(c.os_version.as_deref(), Some("11 (15.0.0)"));
    assert!(c.client_hints_used);
    // garbage hint values change nothing
    let c = eng().classify_client(&req(
        URL,
        None,
        CHROME_MAC,
        &[
            ("Sec-CH-UA-Platform", "\"Plan9\""),
            ("Sec-CH-UA-Mobile", "?7"),
        ],
    ));
    assert_eq!(c.os, Os::Macos);
    assert_eq!(c.form_factor, FormFactor::Desktop);
}

/// Specific platforms from the UA (TV, console, XR) survive a generic platform hint.
#[test]
fn client_hints_do_not_flatten_specific_platforms() {
    let c = eng().classify_client(&req(
        URL,
        None,
        QUEST3,
        &[("Sec-CH-UA-Platform", "\"Android\"")],
    ));
    assert_eq!(c.form_factor, FormFactor::XrHeadset);
}

// ------------------------------------------------------------------ Android apps, Custom Tabs

#[test]
fn android_app_referrers() {
    for (r, ch, src) in [
        (
            "android-app://com.google.android.gm/",
            Channel::Email,
            "gmail",
        ),
        (
            "android-app://com.reddit.frontpage/",
            Channel::OrganicSocial,
            "reddit",
        ),
        (
            "android-app://com.google.android.googlequicksearchbox/",
            Channel::OrganicSearch,
            "google",
        ),
        (
            "android-app://com.twitter.android",
            Channel::OrganicSocial,
            "x",
        ),
    ] {
        let a = at(URL, Some(r), CHROME_AND, &[]);
        top(&a, ch, src);
        assert_eq!(a.client.surface, Surface::CustomTab, "{r}");
    }
    // an own package is a Trusted Web Activity
    let mut r = req(URL, Some("android-app://com.shop.app/"), CHROME_AND, &[]);
    r.self_android_packages = vec!["com.shop.app".into()];
    assert_eq!(eng().attribute(&r).client.surface, Surface::Twa);
}

/// A referrer that is not a package name names no app.
#[test]
fn malformed_android_app_referrers_are_no_referrer() {
    for r in [
        "android-app://",
        "android-app:///",
        "android-app://-/",
        "android-app://1abc.def/",
        "android-app://single/",
    ] {
        let a = at(URL, Some(r), CHROME_AND, &[]);
        top(&a, Channel::Direct, "(direct)");
        assert_eq!(a.client.surface, Surface::StandaloneBrowser, "{r}");
        assert_eq!(a.client.app, None, "{r}");
    }
}

#[test]
fn x_requested_with_must_be_an_app_package() {
    for x in [
        "   ",
        "\"",
        "Windows",
        "XMLHttpRequest",
        "com.android.chrome",
        "../../etc",
        "a b.c",
    ] {
        let a = at(URL, None, CHROME_AND, &[("X-Requested-With", x)]);
        top(&a, Channel::Direct, "(direct)");
        assert_eq!(a.client.app, None, "{x:?}");
    }
    let a = at(
        URL,
        None,
        PLAIN_WV,
        &[("X-Requested-With", "com.example.unknown_app")],
    );
    top(&a, Channel::OrganicSocial, "com.example.unknown_app");
    let a = at(
        URL,
        None,
        PLAIN_WV,
        &[("X-Requested-With", "com.google.android.gm")],
    );
    assert_eq!(a.client.surface, Surface::EmailClient);
}

// ------------------------------------------------------------------ iOS Link Tracking Protection

/// Safari's Link Tracking Protection strips click IDs (gclid, fbclid, ...) in Private Browsing
/// and from Mail/Messages links; what remains is the referrer, or nothing.
#[test]
fn ltp_stripped_urls_fall_back_to_the_referrer() {
    top(
        &at(URL, Some("https://www.google.com/"), SAFARI_IOS, &[]),
        Channel::OrganicSearch,
        "google",
    );
    top(
        &at(URL, Some("https://l.facebook.com/"), SAFARI_IOS, &[]),
        Channel::OrganicSocial,
        "facebook",
    );
    top(&at(URL, None, SAFARI_IOS, &[]), Channel::Direct, "(direct)");
    // UTM survives LTP
    top(
        &at(
            &format!("{URL}?utm_source=google&utm_medium=cpc"),
            None,
            SAFARI_IOS,
            &[],
        ),
        Channel::PaidSearch,
        "google",
    );
}

// ------------------------------------------------------------------ malformed parameters

#[test]
fn query_parameter_edge_cases() {
    let t = |q: &str| at(&format!("{URL}?{q}"), None, CHROME_MAC, &[]);
    top(&t("GCLID=1"), Channel::PaidSearch, "google");
    top(&t("gclid="), Channel::PaidSearch, "google");
    top(&t("gclid=1&gclid=2"), Channel::PaidSearch, "google");
    top(
        &t("UTM_SOURCE=Facebook&UTM_MEDIUM=CPC"),
        Channel::PaidSocial,
        "facebook",
    );
    top(
        &t("utm_source=face%62ook&utm_medium=paid%5Fsocial"),
        Channel::PaidSocial,
        "facebook",
    );
    top(
        &t("utm_source=%20Facebook%20&utm_medium=%20CPC"),
        Channel::PaidSocial,
        "facebook",
    );
    top(
        &t("utm_source=+facebook+&utm_medium=social"),
        Channel::OrganicSocial,
        "facebook",
    );
    top(
        &t("utm_source=google&utm_source=facebook&utm_medium=cpc"),
        Channel::PaidSearch,
        "google",
    );
    // a blank value is no value
    top(
        &t("utm_source=%20&utm_medium="),
        Channel::Direct,
        "(direct)",
    );
    // `&amp;` leaves `amp;utm_medium`, which is not utm_medium
    top(
        &t("utm_source=facebook&amp;utm_medium=cpc"),
        Channel::OrganicSocial,
        "facebook",
    );
    // `;` is not a separator (WHATWG URL), so the source is the whole value
    assert_eq!(
        t("utm_source=facebook;utm_medium=cpc").top().channel,
        Channel::Referral
    );
    // broken escapes are kept literally rather than rejected
    let a = t("utm_source=%zz&utm_medium=%");
    assert_eq!(a.top().source, "%zz");
    // tags in the fragment never reach the server
    top(
        &at(
            &format!("{URL}#utm_source=facebook&utm_medium=cpc"),
            None,
            CHROME_MAC,
            &[],
        ),
        Channel::Direct,
        "(direct)",
    );
}

#[test]
fn idn_and_punycode_hosts() {
    // a lookalike of google in punycode is not google
    let a = at(URL, Some("https://xn--ggle-0nda.com/"), CHROME_MAC, &[]);
    assert_eq!(a.top().channel, Channel::Referral);
    let a = at(URL, Some("https://bücher.de/"), CHROME_MAC, &[]);
    top(&a, Channel::Referral, "xn--bcher-kva.de");
    // a Unicode site and its punycode referrer are the same site
    let a = at(
        "https://bücher.example/p",
        Some("https://xn--bcher-kva.example/x"),
        CHROME_MAC,
        &[],
    );
    top(&a, Channel::Internal, "(self)");
}

#[test]
fn amp_cache_and_viewer() {
    let a = at(
        URL,
        Some("https://shop-example.cdn.ampproject.org/c/s/shop.example/p"),
        CHROME_AND,
        &[],
    );
    assert_eq!(a.client.surface, Surface::AmpCache);
    top(
        &at(
            URL,
            Some("https://www.google.com/amp/s/shop.example/p"),
            CHROME_AND,
            &[],
        ),
        Channel::OrganicSearch,
        "google",
    );
    top(
        &at(
            "https://shop-example.cdn.ampproject.org/c/s/shop.example/p",
            Some("https://www.google.com/"),
            CHROME_AND,
            &[],
        ),
        Channel::OrganicSearch,
        "google",
    );
}

// ------------------------------------------------------------------ email

#[test]
fn webmail_and_mail_clients() {
    for (r, src) in [
        ("https://mail.google.com/mail/u/0/", "gmail"),
        ("https://outlook.live.com/", "webmail"),
        ("https://mail.yahoo.com/", "webmail"),
        ("https://mail.proton.me/", "webmail"),
    ] {
        let a = at(URL, Some(r), CHROME_MAC, &[]);
        top(&a, Channel::Email, src);
        assert_eq!(a.client.surface, Surface::Webmail, "{r}");
    }
    // the mail client's own UA with no referrer came out of email
    for (ua, app) in [(OUTLOOK, "outlook"), (THUNDERBIRD, "thunderbird")] {
        let a = at(URL, None, ua, &[]);
        top(&a, Channel::Email, app);
        assert!((a.top().confidence - weights::EMAIL_CLIENT_NO_REFERRER).abs() < 1e-9);
        assert!(has(&a, Channel::Direct, "(direct)"));
        assert_eq!(a.client.surface, Surface::EmailClient);
        assert_eq!(a.client.app.as_deref(), Some(app));
    }
    // the Gmail app's WebView names itself
    let a = at(
        URL,
        None,
        PLAIN_WV,
        &[("X-Requested-With", "com.google.android.gm")],
    );
    top(&a, Channel::Email, "gmail");
}

// ------------------------------------------------------------------ in-app browsers

#[test]
fn other_in_app_browsers_without_referrer() {
    for (ua, src) in [
        (FB_AND, "facebook"),
        (IG_IOS, "instagram"),
        (TIKTOK_IOS, "tiktok"),
        (KAKAO_IOS, "kakaotalk"),
        (LINE_AND, "line"),
        (SNAP_IOS, "snapchat"),
        (TELEGRAM_AND, "telegram"),
    ] {
        let a = at(URL, None, ua, &[]);
        assert_eq!(a.top().source, src, "{ua}: {}", summary(&a));
        assert_ne!(a.top().channel, Channel::Direct);
        assert!(a.new_touch);
    }
    // Google app WebView: the Google app, a search surface
    let a = at(URL, None, GSA_AND, &[]);
    assert_eq!(a.top().source, "google", "{}", summary(&a));
}

// ------------------------------------------------------------------ devices

#[test]
fn consoles_tvs_headsets_and_cars() {
    for (ua, ff) in [
        (QUEST3, FormFactor::XrHeadset),
        // CrKey on Fuchsia is a Nest Hub
        (CHROMECAST, FormFactor::SmartDisplay),
        (WEBOS, FormFactor::Tv),
        (ROKU, FormFactor::Tv),
        (PS5, FormFactor::Console),
        (XBOX, FormFactor::Console),
        (FIRETV, FormFactor::Tv),
        (TESLA, FormFactor::Car),
    ] {
        let a = at(URL, None, ua, &[]);
        assert_eq!(a.client.form_factor, ff, "{ua}");
        assert!(a.bot.is_none(), "{ua}");
        top(&a, Channel::Direct, "(direct)");
    }
}

// ------------------------------------------------------------------ garbage and size

#[test]
fn garbage_headers_and_referrers() {
    for r in [
        "null",
        "about:blank",
        "/foo",
        "   ",
        "javascript:alert(1)",
        "file:///C:/x",
        "data:text/html,x",
        "::::",
        "-",
        "http://",
        "https://[::1",
    ] {
        let a = at(URL, Some(r), CHROME_MAC, &[]);
        top(&a, Channel::Direct, "(direct)");
    }
    let a = at(
        URL,
        None,
        CHROME_MAC,
        &[
            ("Sec-Fetch-Site", "\u{0}"),
            ("Referer", "::::"),
            ("X-Requested-With", "   "),
            ("Sec-Fetch-Dest", "\u{7f}"),
        ],
    );
    assert_eq!(a.top().channel, Channel::Direct);
    assert_eq!(a.client.surface, Surface::StandaloneBrowser);
    // a whitespace-only UA is as empty as no UA
    for ua in ["", " ", "\t"] {
        assert_eq!(
            at(URL, None, ua, &[]).bot.unwrap().class,
            BotClass::Automation
        );
    }
    let mut r = req(URL, None, CHROME_MAC, &[]);
    r.user_agent = None;
    assert!(eng().attribute(&r).bot.is_some());
    // URLs that do not parse still attribute
    for u in ["", "::::", "/p?gclid=1", "shop.example/p"] {
        let a = at(u, None, CHROME_MAC, &[]);
        assert!(!a.candidates.is_empty(), "{u:?}");
    }
    top(
        &at("/p?gclid=1", None, CHROME_MAC, &[("Host", "shop.example")]),
        Channel::PaidSearch,
        "google",
    );
}

#[test]
fn very_long_inputs_are_handled_without_blowup() {
    let long_q = format!("{URL}?{}gclid=1", "a=1&".repeat(5000));
    top(
        &at(&long_q, None, CHROME_MAC, &[]),
        Channel::PaidSearch,
        "google",
    );
    let long_ua = format!("Mozilla/5.0 {} Twitter for iPhone/1", "x".repeat(200_000));
    top(&at(URL, None, &long_ua, &[]), Channel::OrganicSocial, "x");
    let long_ref = format!("https://www.google.com/{}", "a".repeat(100_000));
    top(
        &at(URL, Some(&long_ref), CHROME_MAC, &[]),
        Channel::OrganicSearch,
        "google",
    );
    let many: Vec<(String, String)> = (0..2000)
        .map(|i| (format!("X-H{i}"), "v".repeat(100)))
        .collect();
    let h: Vec<(&str, &str)> = many.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    top(&at(URL, None, CHROME_MAC, &h), Channel::Direct, "(direct)");
    let a = at(
        &format!("{URL}?utm_source={}", "f".repeat(50_000)),
        None,
        CHROME_MAC,
        &[],
    );
    assert_eq!(a.top().source.len(), 50_000);
    assert!(a.to_json().len() > 50_000);
}
