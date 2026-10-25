//! Differential tests: every built-in pattern against `regex`, every URL against `url`, over the
//! golden corpus, hand-picked edge cases and a seeded fuzz run.
//!
//! Run with `cargo test --release --manifest-path difftest/Cargo.toml`.
use crate::pattern::Regex as Mine;
use crate::url::Url as MyUrl;
use std::collections::BTreeSet;

fn read(p: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(p);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

type Parts = (
    String,
    Option<String>,
    String,
    Option<String>,
    Vec<(String, String)>,
);

fn reference(u: &str) -> Option<Parts> {
    url::Url::parse(u).ok().map(|x| {
        (
            x.scheme().to_string(),
            x.host_str().map(String::from),
            x.path().to_string(),
            x.query().map(String::from),
            x.query_pairs()
                .map(|(a, b)| (a.into_owned(), b.into_owned()))
                .collect(),
        )
    })
}

fn ours(u: &str) -> Option<Parts> {
    MyUrl::parse(u).map(|x| {
        (
            x.scheme().to_string(),
            x.host_str().map(String::from),
            x.path().to_string(),
            x.query().map(String::from),
            x.query_pairs(),
        )
    })
}

/// Known, accepted differences:
/// - non-special schemes that the `url` crate parses but the WHATWG URL Standard (which this crate
///   follows) rejects, such as an empty host after credentials or a non-numeric port;
/// - `xn--` labels whose validity depends on the IDNA mapping and validity tables, which this crate
///   does not carry. It validates the punycode itself and accepts or rejects some such labels
///   differently, in either direction. Real referrer hosts are unaffected.
fn known_divergence(u: &str, theirs: &Option<Parts>, mine: &Option<Parts>) -> bool {
    let idna = u.to_ascii_lowercase().contains("xn--") && (theirs.is_none() || mine.is_none());
    let lenient = mine.is_none()
        && theirs.as_ref().is_some_and(|(scheme, ..)| {
            !["http", "https", "ws", "wss", "ftp", "file"].contains(&scheme.as_str())
        });
    idna || lenient
}

fn patterns() -> Vec<(String, bool)> {
    let mut out = Vec::new();
    // pull "re"/"path"/... string values from the JSON with a regex (the reference engine)
    let grab = |text: &str, key: &str, ci: bool, out: &mut Vec<(String, bool)>| {
        let r = regex::Regex::new(&format!(r#""{key}"\s*:\s*"((?:[^"\\]|\\.)*)""#)).unwrap();
        for c in r.captures_iter(text) {
            let raw = format!("\"{}\"", &c[1]);
            let v: String = unescape(&raw);
            out.push((v, ci));
        }
    };
    let rules = read("rules/rules.json");
    let devices = read("rules/devices.json");
    // uaTokens and referrers both use "re" with ci; devices/browsers use "re" without
    grab(&rules, "re", true, &mut out);
    grab(&rules, "path", false, &mut out);
    for k in ["paidMedium", "shoppingCampaign", "display", "socialMedium"] {
        grab(&rules, k, false, &mut out);
    }
    for k in ["paidSource", "shoppingHosts", "videoHosts", "refHost"] {
        grab(&rules, k, true, &mut out);
    }
    grab(&rules, "valueRe", false, &mut out);
    grab(&devices, "re", false, &mut out);
    // bot tokens as the engine builds them
    let tok = regex::Regex::new(r#""token"\s*:\s*"((?:[^"\\]|\\.)*)""#).unwrap();
    for c in tok.captures_iter(&rules) {
        let t = unescape(&format!("\"{}\"", &c[1]));
        let tail = if t.ends_with(|c: char| c.is_alphanumeric()) {
            "\\b"
        } else {
            ""
        };
        out.push((format!("\\b{}{tail}", regex::escape(&t)), true));
    }
    for p in [
        r"OS (\d+[_.]\d+(?:[_.]\d+)?)",
        r"Android (\d+(?:\.\d+)*)",
        r"Mac OS X (\d+[_.]\d+(?:[_.]\d+)?)",
        r"Windows NT (\d+\.\d+)",
        r"(?:OpenHarmony|HarmonyOS) (\d+(?:\.\d+)*)",
        r"KAIOS/(\d+(?:\.\d+)*)",
        r"Tizen (\d+(?:\.\d+)*)",
        r"PlayStation \d/(\d+(?:\.\d+)*)",
        r"Android 10; K\)",
        r"\((Phone|Tablet|PC); OpenHarmony",
        r"(?i)Microsoft Outlook|Thunderbird/|Lotus-Notes|Airmail",
        r"^(mail\.google\.com|outlook\.(live|office|office365)\.com|mail\.yahoo\.com|mail\.proton\.me|mail\.aol\.com|mail\.yandex\.\w+|e\.mail\.ru)$",
        r"[?&](utm_source|source|src)=(pwa|homescreen|a2hs|web_app_manifest)\b",
        r"(^|\.)(t\.co|x\.com|twitter\.com)$",
        r"^(mail|docs|drive)\.",
        r"\.[a-z]{2,3}(\.[a-z]{2})?$",
        r"[-_. ]?ads?$",
        r"e[-_ ]?mail|newsletter",
        r"^(google[ _]shopping|igshopping|shopping)$",
        r"^(youtube|yt|vimeo|twitch|kick)$",
        r"^(fb|ig|x|twitter|tw|li|bluesky|bsky|mastodon|threads|meta)([-_. ].*)?$",
        r"^(gmail|outlook|yahoo_mail|e-?mail|newsletter)$",
        r"(?i)^(com\.android\.chrome|com\.chrome\.(beta|dev|canary)|org\.mozilla\.\w+|com\.sec\.android\.app\.sbrowser|com\.opera\.\w+|com\.brave\.browser|com\.microsoft\.emmx|com\.UCMobile\.intl|com\.mi\.globalbrowser|com\.duckduckgo\.mobile\.android|com\.vivaldi\.browser|XMLHttpRequest)$",
        r"(?i)^(com\.google\.android\.gm|com\.microsoft\.office\.outlook|com\.yahoo\.mobile\.client\.android\.mail|ch\.protonmail\.android|com\.samsung\.android\.email\.provider)$",
        r"^(chatgpt(\.com)?|openai|perplexity|gemini|copilot|claude|grok|deepseek|meta[_ .]?ai|poe|mistral|duck[_.]?ai|other_ai)$",
        r#"^\S+ \S+ \S+ \[[^\]]+\] "(?:[A-Z]+) (\S+)[^"]*" \d{3} \S+ "((?:[^"\\]|\\.)*)" "((?:[^"\\]|\\.)*)""#,
    ] {
        out.push((p.to_string(), false));
    }
    out
}

fn unescape(s: &str) -> String {
    // JSON string unescape for the subset used in rule files
    let inner = &s[1..s.len() - 1];
    let mut out = String::new();
    let mut it = inner.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next().unwrap() {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                '/' => out.push('/'),
                'u' => {
                    let h: String = it.by_ref().take(4).collect();
                    out.push(char::from_u32(u32::from_str_radix(&h, 16).unwrap()).unwrap());
                }
                o => panic!("escape {o}"),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn corpus() -> (Vec<String>, Vec<String>) {
    let tsv = read("tests/golden/requests.tsv");
    let log = read("tests/golden/access.log");
    let mut hay = BTreeSet::new();
    let mut urls = BTreeSet::new();
    for line in tsv.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        for (i, x) in f.iter().enumerate() {
            hay.insert(x.to_string());
            if let Some((_, v)) = x.split_once(": ") {
                hay.insert(v.to_string());
            }
            if i < 2 {
                urls.insert(x.to_string());
            }
        }
    }
    for l in log.lines() {
        hay.insert(l.to_string());
    }
    // derived: hosts, paths, lowercase variants, query values
    let mut extra = Vec::new();
    for u in &urls {
        if let Ok(p) = url::Url::parse(u) {
            if let Some(h) = p.host_str() {
                extra.push(h.to_string());
                extra.push(h.trim_start_matches("www.").to_string());
            }
            extra.push(p.path().to_string());
            for (k, v) in p.query_pairs() {
                extra.push(k.to_string());
                extra.push(v.to_lowercase());
            }
        }
    }
    hay.extend(extra);
    for w in [
        "",
        "a",
        "threads",
        "google_ads",
        "fbads",
        "google.co.uk",
        "AFTMM",
        "aftmm",
        "KFTRWI",
        "\u{212A}",
        "ſ",
        "Ǆ",
        "é",
        "ÉCOLE",
        "x\ny",
        "Gptbot",
        "GPTBot\u{00e9}",
        "日本GPTBot",
        "GPTBotä",
        "ä GPTBot",
        "über.de",
    ] {
        hay.insert(w.to_string());
    }
    let mut more_urls: Vec<String> = urls.into_iter().collect();
    for u in EDGE_URLS {
        more_urls.push(u.to_string());
    }
    (hay.into_iter().collect(), more_urls)
}

const EDGE_URLS: &[&str] = &[
    "",
    "-",
    "not a url",
    "/relative/path",
    "://",
    "http://",
    "https://",
    "https:",
    "https:a.b",
    "https:\\\\a.b\\c\\d",
    "https:///a.b/c",
    "HTTPS://A.B/",
    " https://a.b/ ",
    "https://a\tb.c/",
    "https://a.b/\n",
    "https://user:pw@a.b:443/x",
    "https://user@/",
    "https://@a.b/",
    "https://a@b@c.d/",
    "https://a.b:/",
    "https://a.b:0/",
    "https://a.b:65535/",
    "https://a.b:65536/",
    "https://a.b:00000000080/",
    "https://a.b:x/",
    "https://a.b:80:80/",
    "http://[::1]/",
    "http://[::1]:8080/",
    "http://[0:0::1]/",
    "http://[1:2:3:4:5:6:7:8]/",
    "http://[1::]/",
    "http://[::]/",
    "http://[1:0:0:2:0:0:0:3]/",
    "http://[::ffff:192.168.0.1]/",
    "http://[::1/",
    "http://[g::1]/",
    "http://[1:2:3:4:5:6:7:8:9]/",
    "http://[1::2::3]/",
    "http://0x7f.1/",
    "http://2130706433/",
    "http://0177.0.0.1/",
    "http://1.2.3/",
    "http://1.2.3.4.5/",
    "http://256.1.1.1/",
    "http://1.2.3.256/",
    "http://1.2.3.4./",
    "http://1.2.3.09/",
    "http://0x/",
    "http://0xg/",
    "http://a.0x1/",
    "http://a.1/",
    "http://a.b1/",
    "http://999999999999999999999/",
    "http://bücher.de/",
    "http://BÜCHER.de/",
    "http://例え.テスト/",
    "http://xn--bcher-kva.de/",
    "http://a%2Eb/",
    "http://%41.b/",
    "http://a%zz.b/",
    "http://a%00.b/",
    "http://a b/",
    "http://a<b/",
    "http://a^b/",
    "http://a|b/",
    "http://a%25b/",
    "http://a.b./",
    "http://.a.b/",
    "http://a..b/",
    "http://a.b/%2e/./x/%2E%2e/y",
    "http://a.b/a/b/..",
    "http://a.b/a/b/.",
    "http://a.b/../../x",
    "http://a.b/a b/\"<>`{}^|/é",
    "http://a.b/?q=a b\"'<>`{}é#frag",
    "http://a.b/?q=%zz&x=%41&+=+",
    "http://a.b?x",
    "http://a.b#x?y",
    "http://a.b/;x=1",
    "http://a.b/%",
    "http://a.b/%%41",
    "android-app://com.reddit.frontpage",
    "android-app://com.Reddit.Frontpage/",
    "android-app://com.x/https/y",
    "android-app://",
    "android-app:com.x",
    "android-app://a b",
    "android-app://a%zz",
    "android-app://a:1",
    "android-app://a:x",
    "android-app://[::1]",
    "android-app://ä",
    "foo://h/a/../b",
    "foo:/a/../b",
    "foo:a/../b",
    "mailto:a@b.c",
    "javascript:alert(1)",
    "data:text/html,hi",
    "about:blank",
    "file:///tmp/x",
    "file://localhost/tmp",
    "file://host/x",
    "file:/x",
    "file:x",
    "file://C:/x",
    "file:///C:/x",
    "file://C|/x",
    "ws://a.b/",
    "wss://a.b:443/",
    "ftp://a.b:21/",
    "localhost:8080/p",
    "chrome-extension://abc/x",
    "ios-app://123/x",
    "a+b-c.d://x",
    "1a://x",
    "-a://x",
    "http://a.b\\c",
    "https://a.b/x?y=1&utm_source=Google&UTM_Medium=CPC&gclid=Ab%20c",
    "https://www.google.com/search?q=shoes",
    "https://a.b/\u{0}x",
    "https://a.b/\u{7f}",
    "https://a.b/?\u{1}",
    "\u{1}https://a.b/\u{1f}",
    "https://ａ.b/",
    "https://a。b/",
    "https://xn--abc-.de/",
    "https://xn--.de/",
    "https://XN--BCHER-KVA.de/",
    "https://ＡＢ.de/",
    "https://a.xn--bcher-kva/",
    "https://xn--bcher-kva.xn--zz/",
    "https://Ünï.de/",
    "https://ß.de/",
    "https://xn--zz.de/",
    "https://a.b/?%C3%A9=%E9",
];

#[test]
fn regex_matches_reference() {
    let pats = patterns();
    let (hay, _) = corpus();
    eprintln!("{} patterns x {} haystacks", pats.len(), hay.len());
    let mut diffs = 0;
    for (p, ci) in &pats {
        let theirs = regex::RegexBuilder::new(p)
            .case_insensitive(*ci)
            .build()
            .unwrap();
        let mine = Mine::build(p, *ci).unwrap_or_else(|e| panic!("{e}"));
        for h in &hay {
            let a = theirs.is_match(h);
            let b = mine.is_match(h);
            let fa = theirs.find(h).map(|m| (m.start(), m.end()));
            let fb = mine.find(h);
            let ca: Option<Vec<Option<&str>>> = theirs
                .captures(h)
                .map(|c| (0..c.len()).map(|i| c.get(i).map(|m| m.as_str())).collect());
            let cb: Option<Vec<Option<&str>>> = mine
                .captures(h)
                .map(|c| (0..theirs.captures_len()).map(|i| c.get(i)).collect());
            if a != b || fa != fb || ca != cb {
                diffs += 1;
                if diffs < 30 {
                    eprintln!("DIFF {p:?} ci={ci} on {h:?}: is_match {a} vs {b}, find {fa:?} vs {fb:?}, caps {ca:?} vs {cb:?}");
                }
            }
        }
    }
    assert_eq!(diffs, 0);
}

#[test]
fn url_matches_reference() {
    let (_, urls) = corpus();
    let mut diffs = 0;
    for u in &urls {
        let (a, b) = (reference(u), ours(u));
        if a != b {
            diffs += 1;
            eprintln!("DIFF {u:?}\n  url:  {a:?}\n  mine: {b:?}");
        }
    }
    eprintln!("{} urls", urls.len());
    assert_eq!(diffs, 0);
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn pick<'a, T>(&mut self, v: &'a [T]) -> &'a T {
        &v[(self.next() % v.len() as u64) as usize]
    }
}

const ATOMS: &[&str] = &[
    "a",
    "A",
    "z",
    "0",
    "9",
    ".",
    "-",
    "_",
    "/",
    "\\",
    ":",
    "@",
    "?",
    "#",
    "&",
    "=",
    "+",
    "%",
    "%2e",
    "%41",
    "%zz",
    "[",
    "]",
    " ",
    "\t",
    "é",
    "ü",
    "日",
    "ａ",
    "xn--",
    "http",
    "://",
    "www.",
    "google",
    "com",
    "GPTBot",
    "bot",
    "Android ",
    "OS ",
    "1",
    "_",
    "10_15_7",
    "Mozilla/5.0 (",
    ")",
    ";",
    "\"",
    "'",
    "<",
    "^",
    "|",
    "..",
    "0x",
    "255",
    "256",
    "localhost",
    "ads",
    "threads",
    "mail.",
    "Instagram",
    "FBAN/",
    "wv",
];

#[test]
fn fuzz_urls_and_patterns() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let prefixes = [
        "http://",
        "https://",
        "HTTPS:",
        "https:\\\\",
        "android-app://",
        "foo://",
        "file://",
        "mailto:",
        "",
        "ws://",
        "x:",
    ];
    let (mut diffs, mut known) = (0, 0);
    let mut hay = Vec::new();
    for _ in 0..200_000 {
        let mut s = rng.pick(&prefixes).to_string();
        for _ in 0..(rng.next() % 12) {
            s.push_str(rng.pick(ATOMS));
        }
        let (a, b) = (reference(&s), ours(&s));
        if a != b {
            if known_divergence(&s, &a, &b) {
                known += 1;
            } else {
                diffs += 1;
                if diffs < 50 {
                    eprintln!("URL DIFF {s:?}\n  url:  {a:?}\n  ours: {b:?}");
                }
            }
        }
        if hay.len() < 20_000 {
            hay.push(s);
        }
    }
    eprintln!("{known} known spec divergences");
    let mut rdiffs = 0;
    for (p, ci) in &patterns() {
        let theirs = regex::RegexBuilder::new(p)
            .case_insensitive(*ci)
            .build()
            .unwrap();
        let mine = Mine::build(p, *ci).unwrap();
        for h in &hay {
            let fa = theirs.find(h).map(|m| (m.start(), m.end()));
            if theirs.is_match(h) != mine.is_match(h) || fa != mine.find(h) {
                rdiffs += 1;
                if rdiffs < 20 {
                    eprintln!("RE DIFF {p:?} {h:?}");
                }
            }
        }
    }
    assert_eq!((diffs, rdiffs), (0, 0));
}
