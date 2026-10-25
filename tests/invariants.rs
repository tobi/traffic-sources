//! Property tests: invariants that must hold for every request, checked over a seeded stream of
//! generated requests that mix real values, near-misses and junk. Deterministic; set
//! `INVARIANT_CASES` to run more (`INVARIANT_CASES=200000 cargo test --release --test invariants`).
use std::collections::{BTreeMap, HashSet};
use traffic_sources::json::{ToJson, Value};
use traffic_sources::*;

/// xorshift64*: enough for test generation, no dependency.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.next() % 100 < pct
    }
    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[self.below(xs.len())]
    }
    fn junk(&mut self, max: usize) -> String {
        const ALPHABET: &[char] = &[
            'a', 'Z', '0', '.', '/', ':', '%', '&', '=', '?', '#', ';', ' ', '\t', '"', '\'', '-',
            '_', '+', '[', ']', '@', '\u{0}', '\u{7f}', 'é', 'ß', '中', '🚀', '\u{200b}',
            '\u{fffd}',
        ];
        let n = self.below(max + 1);
        (0..n)
            .map(|_| ALPHABET[self.below(ALPHABET.len())])
            .collect()
    }
}

// Real values (sources in tests/adversarial.rs and tests/sourced.rs) and near-misses.
const UAS: &[&str] = &[
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Mobile Safari/537.36",
    "Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.6 Mobile/15E148 Safari/604.1",
    "Mozilla/5.0 (X11; Linux x86_64; rv:144.0) Gecko/20100101 Firefox/144.0",
    "Mozilla/5.0 (iPhone; CPU iPhone OS 18_6_2 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/22G100 Twitter for iPhone/12.31.2",
    "Mozilla/5.0 (Linux; Android 16; 23078PND5G Build/BP2A.250605.031.A3; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/153.0.8010.39 Mobile Safari/537.36 TwitterAndroid/12.32.0-prod.01 (312320001-r-01) 23078PND5G/16 (Xiaomi;23078PND5G;Xiaomi;corot_global;0;;1;2015)",
    "Mozilla/5.0 (iPad; U; CPU OS 4_3_5 like Mac OS X; en-us) AppleWebKit/533.17.9 (KHTML, like Gecko) Mobile/8L1 Twitter for iPad",
    "Mozilla/5.0 (Linux; Android 5.1.1; Nexus 5 Build/LMY48B; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/43.0.2357.65 Mobile Safari/537.36",
    "Mozilla/5.0 (Linux; Android 5.1.1; R7sf Build/LMY47V; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/80.0.3987.149 Mobile Safari/537.36 [FB_IAB/FB4A;FBAV/263.0.0.46.121;]",
    "Mozilla/5.0 (iPhone; CPU iPhone OS 11_2_5 like Mac OS X) AppleWebKit/604.5.6 (KHTML, like Gecko) Mobile/15D60 Instagram 33.0.0.11.96 (iPhone9,3; iOS 11_2_5; en_AU; en-AU; scale=2.00; gamut=wide; 750x1334)",
    "Microsoft Office/14.0 (Windows NT 6.1; Microsoft Outlook 14.0.5128; Pro)",
    "Mozilla/5.0 (X11; Linux x86_64; Quest 3) AppleWebKit/537.36 (KHTML, like Gecko) OculusBrowser/26.2.0.0.10 Chrome/110.0.5481.192 VR Safari/537.36",
    "Mozilla/5.0 (PlayStation 5 3.11) AppleWebKit/605.1.15 (KHTML, like Gecko)",
    "Roku/DVP-5.0 (025.00E08043A)",
    "Twitterbot/1.0",
    "facebookexternalhit/1.1",
    "WhatsApp",
    "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko); compatible; ChatGPT-User/1.0; +https://openai.com/bot",
    "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; GPTBot/1.0; +https://openai.com/gptbot)",
    "Claude-User (claude-code/2.1.86; +https://support.anthropic.com/)",
    "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)",
    "curl/8.5.0",
    "",
    " ",
    "Mozilla/5.0",
    "Twitter for iPhone",
    "Instagram",
];
const REFS: &[&str] = &[
    "https://www.google.com/",
    "https://www.google.com/search?q=x&udm=50",
    "https://t.co/abc",
    "https://l.facebook.com/l.php?u=x",
    "https://lnkd.in/x",
    "https://news.ycombinator.com/item?id=1",
    "https://mastodon.social/@a",
    "https://chatgpt.com/",
    "https://mail.google.com/mail/u/0/",
    "https://shop.example/cart",
    "https://www.shop.example./",
    "https://evilshop.example/",
    "android-app://com.google.android.gm/",
    "android-app://com.reddit.frontpage",
    "android-app://",
    "https://shop-example.cdn.ampproject.org/c/s/shop.example/p",
    "https://bücher.de/",
    "about:blank",
    "null",
    "-",
    "::::",
    "https://[::1]:8080/",
    "http://192.168.0.1/",
];
const PARAMS: &[&str] = &[
    "utm_source",
    "utm_medium",
    "utm_campaign",
    "UTM_SOURCE",
    "gclid",
    "GCLID",
    "gbraid",
    "wbraid",
    "fbclid",
    "msclkid",
    "ttclid",
    "twclid",
    "li_fat_id",
    "ScCid",
    "igshid",
    "s",
    "t",
    "ref",
    "mc_cid",
    "_hsenc",
    "srsltid",
    "amp;utm_medium",
    "",
];
const VALUES: &[&str] = &[
    "google",
    "facebook",
    "x",
    "newsletter",
    "chatgpt.com",
    "cpc",
    "email",
    "social",
    "paid_social",
    "organic",
    "(direct)",
    "%20Facebook%20",
    "%zz",
    "1",
    "",
    "+",
    "a%26b",
    "20",
];
const HEADER_NAMES: &[&str] = &[
    "Sec-Fetch-Site",
    "Sec-Fetch-Dest",
    "Sec-Fetch-Mode",
    "Sec-Fetch-User",
    "Sec-Purpose",
    "Purpose",
    "X-Purpose",
    "X-Moz",
    "X-Requested-With",
    "Sec-CH-UA-Platform",
    "Sec-CH-UA-Mobile",
    "Sec-CH-UA-Model",
    "Sec-CH-UA-Form-Factors",
    "Sec-CH-UA-Platform-Version",
    "Referer",
    "Host",
    "User-Agent",
    "Signature-Agent",
    "X-Forwarded-For",
    "Via",
];
const HEADER_VALUES: &[&str] = &[
    "cross-site",
    "same-site",
    "same-origin",
    "none",
    "document",
    "iframe",
    "frame",
    "embed",
    "object",
    "image",
    "empty",
    "script",
    "navigate",
    "no-cors",
    "?1",
    "?0",
    "prefetch",
    "prefetch;prerender",
    "com.twitter.android",
    "com.google.android.gm",
    "com.android.chrome",
    "\"Android\"",
    "\"Windows\"",
    "\"macOS\"",
    "\"Pixel 9\"",
    "\"XR\"",
    "\"15.0.0\"",
    "\"https://chatgpt.com\"",
    "shop.example",
    "",
    "   ",
];

fn gen(rng: &mut Rng) -> Request {
    let host = rng.pick(&[
        "shop.example",
        "www.shop.example",
        "bücher.example",
        "SHOP.EXAMPLE",
    ]);
    let mut url = match rng.below(10) {
        0 => String::new(),
        1 => "/p".into(),
        2 => rng.junk(40),
        _ => format!("https://{host}/p"),
    };
    if rng.chance(70) {
        let n = rng.below(5);
        for i in 0..n {
            url.push(if i == 0 { '?' } else { '&' });
            url.push_str(rng.pick(PARAMS));
            if rng.chance(90) {
                url.push('=');
                url.push_str(&if rng.chance(85) {
                    rng.pick(VALUES).to_string()
                } else {
                    rng.junk(12)
                });
            }
        }
    }
    if rng.chance(5) {
        url.push_str("#utm_source=facebook");
    }
    let referer = match rng.below(10) {
        0..=3 => None,
        4 => Some(rng.junk(30)),
        _ => Some(rng.pick(REFS).to_string()),
    };
    let user_agent = match rng.below(20) {
        0 => None,
        1 => Some(rng.junk(60)),
        2 => Some(format!("{} {}", rng.pick(UAS), rng.junk(20))),
        _ => Some(rng.pick(UAS).to_string()),
    };
    let mut headers = BTreeMap::new();
    for _ in 0..rng.below(6) {
        let name = rng.pick(HEADER_NAMES).to_string();
        let v = if rng.chance(85) {
            rng.pick(HEADER_VALUES).to_string()
        } else {
            rng.junk(20)
        };
        headers.insert(name, v);
    }
    let self_hosts = if rng.chance(15) {
        vec!["shop.example".to_string(), rng.junk(10)]
    } else {
        vec![]
    };
    Request {
        url,
        referer,
        user_agent,
        headers,
        self_hosts,
        self_android_packages: if rng.chance(5) {
            vec!["com.shop.app".into()]
        } else {
            vec![]
        },
    }
}

fn cases() -> usize {
    std::env::var("INVARIANT_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3000)
}
fn eng() -> &'static Attributor {
    static E: std::sync::OnceLock<Attributor> = std::sync::OnceLock::new();
    E.get_or_init(Attributor::new)
}

fn for_each(seed: u64, mut f: impl FnMut(&Request, &Attribution)) {
    let mut rng = Rng(seed);
    for i in 0..cases() {
        let r = gen(&mut rng);
        let a = eng().attribute(&r);
        let ctx = format!("case {i} seed {seed}: {r:?}");
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&r, &a)));
        if let Err(e) = res {
            let msg = e.downcast_ref::<String>().cloned().unwrap_or_default();
            panic!("{msg}\n{ctx}\n{}", a.to_json());
        }
    }
}

#[test]
fn candidates_are_well_formed() {
    for_each(1, |_, a| {
        assert!(!a.candidates.is_empty());
        let mut seen = HashSet::new();
        let mut share = 0.0;
        for c in &a.candidates {
            assert!(
                c.confidence.is_finite() && (0.0..=1.0).contains(&c.confidence),
                "{}",
                c.confidence
            );
            assert!(c.share.is_finite() && (0.0..=1.0).contains(&c.share));
            share += c.share;
            assert!(
                seen.insert((c.channel, c.source.clone())),
                "duplicate {}/{}",
                c.channel,
                c.source
            );
            assert!(
                !c.evidence.is_empty(),
                "{}/{} has no evidence",
                c.channel,
                c.source
            );
            for &i in &c.evidence {
                assert!(i < a.evidence.len());
            }
            assert_eq!(c.paid == Paid::Paid, c.channel.paid() == Paid::Paid);
        }
        assert!((share - 1.0).abs() < 0.01, "shares sum to {share}");
        for w in a.candidates.windows(2) {
            assert!(w[0].confidence >= w[1].confidence, "not sorted");
        }
        for e in &a.evidence {
            assert!(e.weight.is_finite() && (0.0..=1.0).contains(&e.weight));
        }
    });
}

#[test]
fn new_touch_is_false_for_non_visits() {
    for_each(2, |_, a| {
        if a.bot.is_some() || a.is_prefetch {
            assert!(!a.new_touch);
        }
        if a.candidates
            .iter()
            .any(|c| matches!(c.interaction, Interaction::Embed | Interaction::Subresource))
        {
            assert!(!a.new_touch);
        }
        if a.top().channel == Channel::Internal {
            assert!(!a.new_touch);
        }
        if a.is_prefetch {
            assert!(a
                .candidates
                .iter()
                .all(|c| c.interaction == Interaction::Prefetch));
        }
    });
}

#[test]
fn bots_are_bots_everywhere() {
    for_each(3, |r, a| {
        let b = eng().classify_bot(r);
        assert_eq!(b.is_some(), a.bot.is_some());
        if let (Some(x), Some(y)) = (&b, &a.bot) {
            assert_eq!(
                (x.class, &x.token, &x.operator, x.exclude),
                (y.class, &y.token, &y.operator, y.exclude)
            );
            assert_eq!(a.client.surface, Surface::Bot);
            assert_eq!(a.client.app, None);
            assert_eq!(a.top().channel, Channel::Bot);
            assert!(a
                .candidates
                .iter()
                .all(|c| matches!(c.channel, Channel::Bot | Channel::AiAssistant)));
        } else {
            assert_ne!(a.client.surface, Surface::Bot);
            assert!(a.candidates.iter().all(|c| c.channel != Channel::Bot));
        }
    });
}

#[test]
fn client_matches_classify_client() {
    for_each(4, |r, a| {
        let c = eng().classify_client(r);
        assert_eq!(
            (c.os, c.form_factor, &c.os_version, &c.browser),
            (
                a.client.os,
                a.client.form_factor,
                &a.client.os_version,
                &a.client.browser
            )
        );
        if a.bot.is_none() {
            assert_eq!((c.surface, &c.app), (a.client.surface, &a.client.app));
        }
        if let Some(app) = &a.client.app {
            assert!(!app.trim().is_empty(), "blank app");
        }
    });
}

#[test]
fn output_is_deterministic_and_valid_json() {
    for_each(5, |r, a| {
        let j = a.to_json();
        assert_eq!(j, eng().attribute(r).to_json());
        let v = Value::parse(&j).expect("attribution JSON parses");
        let n = v
            .get("candidates")
            .and_then(Value::as_array)
            .map(<[Value]>::len);
        assert_eq!(n, Some(a.candidates.len()));
    });
}

#[test]
fn request_json_round_trips() {
    for_each(6, |r, a| {
        let back = Request::from_json(&r.to_json()).expect("request JSON reparses");
        assert_eq!(eng().attribute(&back).to_json(), a.to_json());
    });
}

#[test]
fn header_name_case_does_not_matter() {
    for_each(7, |r, a| {
        let mut up = r.clone();
        up.headers = r
            .headers
            .iter()
            .map(|(k, v)| (k.to_ascii_uppercase(), v.clone()))
            .collect();
        let mut low = r.clone();
        low.headers = r
            .headers
            .iter()
            .map(|(k, v)| (k.to_ascii_lowercase(), v.clone()))
            .collect();
        // differently-cased duplicates collapse differently, so only compare when names are unique
        if up.headers.len() == r.headers.len() {
            assert_eq!(eng().attribute(&up).to_json(), a.to_json());
            assert_eq!(eng().attribute(&low).to_json(), a.to_json());
        }
    });
}

/// Surrounding whitespace in a header value is not part of it (RFC 9110 5.5).
#[test]
fn header_value_padding_does_not_matter() {
    for_each(8, |r, a| {
        let mut padded = r.clone();
        padded.headers = r
            .headers
            .iter()
            .map(|(k, v)| (k.clone(), format!(" {v}\t")))
            .collect();
        assert_eq!(eng().attribute(&padded).to_json(), a.to_json());
    });
}

/// Explicit `referer`/`user_agent` fields and the same values as headers are one request.
#[test]
fn fields_and_headers_are_interchangeable() {
    for_each(9, |r, a| {
        if r.headers.keys().any(|k| {
            k.eq_ignore_ascii_case("referer")
                || k.eq_ignore_ascii_case("user-agent")
                || k.eq_ignore_ascii_case("referrer")
        }) {
            return;
        }
        let mut h = r.clone();
        h.referer = None;
        h.user_agent = None;
        if let Some(x) = &r.referer {
            h.headers.insert("Referer".into(), x.clone());
        }
        if let Some(x) = &r.user_agent {
            h.headers.insert("User-Agent".into(), x.clone());
        }
        // an explicit empty field and an absent header differ only in "empty means absent"
        if r.referer.as_deref() == Some("")
            || r.user_agent.as_deref().is_some_and(|u| u.trim().is_empty())
        {
            return;
        }
        assert_eq!(eng().attribute(&h).to_json(), a.to_json());
    });
}

/// Attribution never panics on byte-level junk in any field.
#[test]
fn pure_junk_never_panics() {
    let mut rng = Rng(0xDEAD_BEEF);
    for _ in 0..cases() {
        let mut headers = BTreeMap::new();
        for _ in 0..rng.below(4) {
            headers.insert(rng.junk(12), rng.junk(40));
        }
        let r = Request {
            url: rng.junk(80),
            referer: Some(rng.junk(80)),
            user_agent: Some(rng.junk(80)),
            headers,
            self_hosts: vec![rng.junk(10)],
            self_android_packages: vec![rng.junk(10)],
        };
        let a = eng().attribute(&r);
        assert!(!a.candidates.is_empty());
        let _ = parse_combined_log(&rng.junk(200), "shop.example");
    }
}
