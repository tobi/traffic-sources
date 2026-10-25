//! Every UA below is copied verbatim from a cited source (no invented strings).
use std::collections::BTreeMap;
use traffic_sources::*;

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
fn eng() -> &'static Attributor {
    static E: std::sync::OnceLock<Attributor> = std::sync::OnceLock::new();
    E.get_or_init(Attributor::new)
}
fn client(ua: &str) -> Client {
    eng().classify_client(&req("https://s.example/", None, ua, &[]))
}

// user-agents.net sample
const X_AND: &str = "Mozilla/5.0 (Linux; Android 16; 23078PND5G Build/BP2A.250605.031.A3; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/153.0.8010.39 Mobile Safari/537.36 TwitterAndroid/12.32.0-prod.01 (312320001-r-01) 23078PND5G/16 (Xiaomi;23078PND5G;Xiaomi;corot_global;0;;1;2015)";
// uap-core / UAParser docs sample
const X_IOS: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_6_2 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/22G100 Twitter for iPhone/12.31.2";

#[test]
fn x_in_app_browser_both_platforms() {
    for (ua, os) in [(X_AND, Os::Android), (X_IOS, Os::Ios)] {
        let a = eng().attribute(&req("https://s.example/p", None, ua, &[]));
        assert_eq!(a.client.surface, Surface::WebviewIab);
        assert_eq!(a.client.app.as_deref(), Some("x"));
        assert_eq!(a.client.os, os);
        assert_eq!(a.client.form_factor, FormFactor::Phone);
        assert_eq!(a.top().channel, Channel::OrganicSocial);
        assert_eq!(a.top().source, "x");
        assert!(a.top().confidence >= 0.8 && a.top().confidence <= 1.0);
    }
}

#[test]
fn paid_click_beats_organic_ua_and_all_candidates_listed() {
    let a = eng().attribute(&req(
        "https://s.example/p?twclid=abc",
        Some("https://t.co/"),
        X_IOS,
        &[],
    ));
    assert_eq!(a.top().channel, Channel::PaidSocial);
    assert_eq!(a.top().paid, Paid::Paid);
    assert!(a
        .candidates
        .iter()
        .any(|c| c.channel == Channel::OrganicSocial && c.source == "x"));
    for c in &a.candidates {
        assert!((0.0..=1.0).contains(&c.confidence));
    }
    let org = a
        .candidates
        .iter()
        .find(|c| c.channel == Channel::OrganicSocial)
        .unwrap();
    assert!(org.confidence < 0.6, "conflict discount");
    let sum: f64 = a.candidates.iter().map(|c| c.share).sum();
    assert!((sum - 1.0).abs() < 0.01);
    assert!(a
        .evidence
        .iter()
        .any(|e| e.rule_src.as_deref().unwrap_or("").contains("http")));
}

#[test]
fn gclid_plus_google_referer_is_paid_search() {
    let a = eng().attribute(&req(
        "https://s.example/?gclid=x&gad_source=1",
        Some("https://www.google.com/"),
        X_IOS,
        &[],
    ));
    assert_eq!(a.top().channel, Channel::PaidSearch);
    assert!(a.top().confidence > 0.95);
}

#[test]
fn utm_ga4_ai_assistant_and_oppref() {
    let a = eng().attribute(&req(
        "https://s.example/?utm_source=chatgpt.com&utm_medium=ai-assistant",
        None,
        X_IOS,
        &[],
    ));
    assert!(a
        .candidates
        .iter()
        .any(|c| c.channel == Channel::AiAssistant && c.source == "chatgpt"));
    let b = eng().attribute(&req("https://s.example/?oppref=abc", Some("https://chatgpt.com/"), "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36", &[]));
    assert_eq!(b.top().channel, Channel::PaidAi);
    assert!(b.client.os_version_frozen);
}

#[test]
fn x_requested_with_webview() {
    let ua = "Mozilla/5.0 (Linux; Android 11; AFTKM) AppleWebKit/537.36 (KHTML, like Gecko) Silk/148.5.11 like Chrome/148.0.7778.280 Safari/537.36";
    let a = eng().attribute(&req(
        "https://s.example/",
        None,
        ua,
        &[("X-Requested-With", "com.instagram.android")],
    ));
    assert_eq!(a.client.surface, Surface::WebviewIab);
    assert_eq!(a.top().source, "instagram");
}

#[test]
fn custom_tab_android_app_referrer() {
    let a = eng().attribute(&req(
        "https://s.example/",
        Some("android-app://com.google.android.gm/"),
        X_AND.split(" TwitterAndroid").next().unwrap(),
        &[],
    ));
    assert!(matches!(
        a.client.surface,
        Surface::WebviewIab | Surface::CustomTab
    ));
    assert_eq!(a.top().channel, Channel::Email);
}

#[test]
fn devices_taxonomy_from_uap_core_and_vendor_docs() {
    let cases: &[(&str, Os, FormFactor)] = &[
        ("Mozilla/5.0 (PlayStation 4 5.00) AppleWebKit/601.2 (KHTML, like Gecko)", Os::Playstation, FormFactor::Console),
        ("Mozilla/5.0 (PlayStation; PlayStation 5/9.60) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.0 Safari/605.1.15", Os::Playstation, FormFactor::Console),
        ("Mozilla/5.0 (compatible; MSIE 10.0; Windows Phone 8.0; Trident/6.0; IEMobile/10.0; Xbox; Xbox One)", Os::Xbox, FormFactor::Console),
        ("Mozilla/5.0 (Windows NT 10.0; Win64; x64; Xbox; Xbox Series X) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/48.0.2564.82 Safari/537.36 Edge/20.02", Os::Xbox, FormFactor::Console),
        ("Mozilla/5.0 (Nintendo Switch; WifiWebAuthApplet) AppleWebKit/609.4 (KHTML, like Gecko) NF/6.0.2.23.6 NintendoBrowser/5.1.0.23620", Os::NintendoSwitch, FormFactor::Console),
        ("Mozilla/5.0 (X11; Linux x86_64; Valve Steam Gamepad/Steam Deck [Steam Deck Stable]/default/0) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.6478.183 Safari/537.36", Os::Steamos, FormFactor::Console),
        ("Mozilla/5.0 (SMART-TV; Linux; Tizen 2.3) AppleWebkit/538.1 (KHTML, like Gecko) SamsungBrowser/1.0 TV Safari/538.1", Os::Tizen, FormFactor::Tv),
        ("Mozilla/5.0 (Web0S; Linux/SmartTV) AppleWebKit/537.41 (KHTML, like Gecko) Large Screen WebAppManager Safari/537.41", Os::Webos, FormFactor::Tv),
        ("Roku/DVP-5.0 (025.00E08043A)", Os::Roku, FormFactor::Tv),
        ("Mozilla/5.0 (Linux; Android 11; AFTKM) AppleWebKit/537.36 (KHTML, like Gecko) Silk/148.5.11 like Chrome/148.0.7778.280 Safari/537.36", Os::FireOs, FormFactor::Tv),
        ("AppleCoreMedia/1.0.0.12F69 (Apple TV; U; CPU OS 8_3 like Mac OS X; en_us)", Os::Tvos, FormFactor::Tv),
        ("Mozilla/5.0 (Linux; Andr0id 12; BRAVIA 4K VH2) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/127.0.6533.120 Safari/537.36 OMI/4.25.1.92.StableAVB_Sony.1", Os::AndroidTv, FormFactor::Tv),
        ("HbbTV/1.2.1 (;Panasonic;VIERA 2013;3.672;4101-0003 0002-0000;)", Os::AndroidTv, FormFactor::Tv),
        ("Mozilla/5.0 (X11; Linux armv7l) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/90.0.4430.225 Safari/537.36 CrKey/1.56.500000 DeviceType/Chromecast", Os::CastOs, FormFactor::SmartDisplay),
        ("Mozilla/5.0 (X11; Linux x86_64; Quest 3) AppleWebKit/537.36 (KHTML, like Gecko) OculusBrowser/36.6.0.9.50.692136875 Chrome/130.0.6723.191 VR Safari/537.36", Os::MetaHorizonOs, FormFactor::XrHeadset),
        ("Mozilla/5.0 (X11; Linux) AppleWebKit/534.34 (KHTML, like Gecko) QtCarBrowser Safari/534.34", Os::Tesla, FormFactor::Car),
        ("atc/1.0 watchOS/5.2 model/Watch4,4 hwp/t8006 build/16T225 (6; dt:193)", Os::Watchos, FormFactor::Wearable),
        ("Mozilla/5.0 (Mobile; rv:48.0) Gecko/48.0 Firefox/48.0 KAIOS/2.5.1", Os::Kaios, FormFactor::Phone),
        ("Mozilla/5.0 (Linux; U; en-US) AppleWebKit/528.5+ (KHTML, like Gecko, Safari/528.5+) Version/4.0 Kindle/3.0 (screen 600x800; rotate)", Os::FireOs, FormFactor::EReader),
        ("Mozilla/5.0 (TAS-AL00 Build/HUAWEITAS-AL00; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/97.0.4692.98 Mobile Safari/537.36 T7/13.76 BDOS/1.0 (HarmonyOS 3.0.0) SP-engine/3.17.0 baiduboxapp/13.76.0.10 (Baidu; P1 12) NABar/1.0", Os::Harmonyos, FormFactor::Phone),
        ("Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148 Safari/604.1 (Brave)", Os::Ipados, FormFactor::Tablet),
        (X_AND, Os::Android, FormFactor::Phone),
    ];
    for (ua, os, ff) in cases {
        let c = client(ua);
        assert_eq!((c.os, c.form_factor), (*os, *ff), "{ua}");
        assert!(c.rule_src.is_some());
    }
    assert_eq!(client(cases[3].0).browser.as_deref(), Some("edge"));
    assert_eq!(client(cases[20].0).browser.as_deref(), Some("brave"));
}

#[test]
fn kindle_fire_tablet_silk() {
    let c = client("Mozilla/5.0 (Linux; U; Android 4.0.3; en-gb; KFTT Build/IML74K) AppleWebKit/537.36 (KHTML, like Gecko) Silk/3.25 like Chrome/34.0.1847.137 Mobile Safari/537.36");
    assert_eq!(c.os, Os::FireOs);
    assert_eq!(c.browser.as_deref(), Some("silk"));
}

#[test]
fn client_hints_override() {
    let ua = "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Mobile Safari/537.36";
    let c = eng().classify_client(&req(
        "https://s.example/",
        None,
        ua,
        &[
            ("Sec-CH-UA-Platform", "\"Android\""),
            ("Sec-CH-UA-Mobile", "?0"),
            ("Sec-CH-UA-Form-Factors", "\"XR\""),
            ("Sec-CH-UA-Platform-Version", "\"15.0.0\""),
            ("Sec-CH-UA-Model", "\"Pixel 9\""),
        ],
    ));
    assert!(c.client_hints_used);
    assert_eq!(c.form_factor, FormFactor::XrHeadset);
    assert_eq!(c.os_version.as_deref(), Some("15.0.0"));
    assert_eq!(c.device.as_deref(), Some("Pixel 9"));
    let w = eng().classify_client(&req("https://s.example/", None, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36", &[("sec-ch-ua-platform", "\"Windows\""), ("sec-ch-ua-platform-version", "\"15.0.0\"")]));
    assert!(w.os_version.unwrap().starts_with("11"));
}

#[test]
fn bots_from_vendor_docs() {
    let cases = [
        ("Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko); compatible; GPTBot/1.4; +https://openai.com/gptbot", BotClass::AiCrawler, true),
        ("Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko); compatible; OAI-AdsBot/1.0; +https://openai.com/adsbot", BotClass::AdVerifier, true),
        ("Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; Perplexity-User/1.0; +https://perplexity.ai/perplexity-user)", BotClass::AiUserFetch, false),
    ];
    for (ua, class, exclude) in cases {
        let a = eng().attribute(&req("https://s.example/", None, ua, &[]));
        let b = a.bot.expect(ua);
        assert_eq!((b.class, b.exclude), (class, exclude), "{ua}");
        assert!(!a.new_touch);
    }
    let a = eng().attribute(&req("https://s.example/", None, "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; Perplexity-User/1.0; +https://perplexity.ai/perplexity-user)", &[]));
    assert!(a
        .candidates
        .iter()
        .any(|c| c.channel == Channel::AiAssistant && c.source == "perplexity"));
    // Web Bot Auth: ChatGPT agent signs with Signature-Agent
    let s = eng().attribute(&req(
        "https://s.example/",
        None,
        X_IOS,
        &[("Signature-Agent", "\"https://chatgpt.com\"")],
    ));
    assert_eq!(s.bot.unwrap().class, BotClass::AiUserFetch);
}

#[test]
fn prefetch_direct_internal_dark_social() {
    let p = eng().attribute(&req(
        "https://s.example/",
        None,
        X_IOS,
        &[("Sec-Purpose", "prefetch;prerender")],
    ));
    assert!(p.is_prefetch && !p.new_touch);
    let d = eng().attribute(&req("https://s.example/", None, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36", &[("Sec-Fetch-Site", "none")]));
    assert_eq!(d.top().channel, Channel::Direct);
    let i = eng().attribute(&req(
        "https://s.example/b",
        Some("https://s.example/a"),
        X_IOS,
        &[],
    ));
    assert_eq!(i.top().channel, Channel::Internal);
    let k = eng().attribute(&req("https://s.example/", None, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36", &[("Sec-Fetch-Site", "cross-site")]));
    assert!(k
        .candidates
        .iter()
        .any(|c| c.channel == Channel::DarkSocial));
}

#[test]
fn referrers_webmail_amp_and_external() {
    let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
    let w = eng().attribute(&req(
        "https://s.example/",
        Some("https://mail.google.com/"),
        ua,
        &[],
    ));
    assert_eq!(w.client.surface, Surface::Webmail);
    assert_eq!(w.top().channel, Channel::Email);
    let amp = eng().classify_client(&req(
        "https://s.example/",
        Some("https://s-example.cdn.ampproject.org/"),
        ua,
        &[],
    ));
    assert_eq!(amp.surface, Surface::AmpCache);
    let e = Attributor::new()
        .with_external_referrers(
            r#"{"referrers":{"news.example.org":{"source":"ExampleNews","medium":"social"}}}"#,
        )
        .unwrap();
    let a = e.attribute(&req(
        "https://s.example/",
        Some("https://news.example.org/x"),
        ua,
        &[],
    ));
    assert_eq!(a.top().channel, Channel::OrganicSocial);
}

#[test]
fn ga4_waterfall() {
    let e = eng();
    assert_eq!(
        e.ga4_channel("google", "cpc", "", "").0,
        Channel::PaidSearch
    );
    assert_eq!(
        e.ga4_channel("facebook", "paid_social", "", "").0,
        Channel::PaidSocial
    );
    assert_eq!(
        e.ga4_channel("newsletter", "email", "", "").0,
        Channel::Email
    );
    assert_eq!(
        e.ga4_channel("(direct)", "(none)", "", "").0,
        Channel::Direct
    );
    assert_eq!(
        e.ga4_channel("x", "y", "cross-network", "").0,
        Channel::CrossNetwork
    );
    assert_eq!(
        e.ga4_channel("perplexity", "referral", "", "").0,
        Channel::AiAssistant
    );
}

#[test]
fn combined_log_parse_and_all_rules_annotated() {
    let r = parse_combined_log(r#"1.2.3.4 - - [24/Oct/2026:10:00:00 -0500] "GET /x?gclid=1 HTTP/1.1" 200 5 "https://www.google.com/" "Roku/DVP-5.0 (025.00E08043A)""#, "s.example").unwrap();
    assert_eq!(r.url, "https://s.example/x?gclid=1");
    let v: serde_json::Value = serde_json::from_str(include_str!("../rules/rules.json")).unwrap();
    for k in ["clickIds", "uaTokens", "referrers", "botRules"] {
        for x in v[k].as_array().unwrap() {
            assert!(
                x.get("src").is_some() && x.get("verified").is_some(),
                "{k}: {x}"
            );
        }
    }
}
