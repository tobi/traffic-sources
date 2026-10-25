//! # traffic-sources
//!
//! Attribute one HTTP request (URL + Referer + User-Agent + headers) to **every plausible
//! marketing channel/source**, each with a confidence in `0.0..=1.0`, plus a client taxonomy
//! (OS, form factor, surface such as in-app WebView or Custom Tab, browser, app) and bot class.
//!
//! Confidence method (see [docs/API.md](https://github.com/tobi/traffic-sources/blob/main/docs/API.md)): every signal `i` is an [`Evidence`] with a likelihood weight
//! `w_i` that names a channel and/or a source. A candidate `(channel, source)` collects every
//! evidence item compatible with it and scores `P = 1 - Π(1 - w_i)` (noisy-OR). Source-only
//! evidence (an in-app UA token) supports every channel for that source; evidence from the same
//! source *family* (e.g. Meta: facebook/instagram/messenger/threads) counts at 0.8×.
//! Scores are independent beliefs, not a distribution; `share` gives the normalised view.
//!
//! All rules are data (`rules/*.json`, embedded with `include_str!`) and each carries `src`
//! (documentation URL) and `verified` (date checked, or null). Shipped data is CC0/MIT/Apache only.
//!
//! ```
//! use traffic_sources::{Attributor, Request};
//!
//! // Compiles every rule set. `Attributor` is Send + Sync — build it once, share it.
//! let attributor = Attributor::new();
//!
//! let req = Request {
//!     url: "https://shop.example/p?twclid=abc".into(),
//!     referer: Some("https://t.co/".into()),
//!     user_agent: Some(
//!         "Mozilla/5.0 (iPhone; CPU iPhone OS 18_6_2 like Mac OS X) AppleWebKit/605.1.15 \
//!          (KHTML, like Gecko) Mobile/22G100 Twitter for iPhone/12.31.2".into(),
//!     ),
//!     self_hosts: vec!["shop.example".into()],
//!     ..Default::default()
//! };
//!
//! let a = attributor.attribute(&req);
//!
//! assert_eq!(a.top().channel.as_str(), "paid_social");
//! assert_eq!(a.top().source, "x");
//! assert_eq!(a.client.surface.as_str(), "webview_iab");
//!
//! // The organic reading survives as a lower-confidence candidate.
//! assert!(a.candidates.iter().any(|c| c.channel.as_str() == "organic_social"));
//! ```

pub mod json;
mod pattern;
mod url;

use json::{ObjectWriter, ToJson, Value};
use pattern::{Haystack, Regex, Screen};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::OnceLock;
use url::Url;

/// Compile a literal regex once.
macro_rules! re {
    ($p:expr) => {{
        static R: OnceLock<Regex> = OnceLock::new();
        R.get_or_init(|| Regex::new($p).expect("built-in pattern"))
    }};
}

const RULES: &str = include_str!("../rules/rules.json");
const DEVICES: &str = include_str!("../rules/devices.json");
const OSS: &str = include_str!("../rules/oss-referrers.json");

// ---------------------------------------------------------------- taxonomy

/// `OrganicSearch` → `organic_search`.
fn snake_case(camel: &str) -> String {
    let mut out = String::with_capacity(camel.len() + 4);
    for (i, c) in camel.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

macro_rules! str_enum {
    ($(#[$m:meta])* $name:ident { $($v:ident),* $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $name { $($v),* }
        impl $name {
            /// Every variant, in declaration order.
            pub const ALL: &'static [$name] = &[$($name::$v),*];
            /// snake_case name, as used in JSON.
            pub fn as_str(&self) -> &'static str {
                static NAMES: OnceLock<Vec<String>> = OnceLock::new();
                &NAMES.get_or_init(|| [$(stringify!($v)),*].iter().map(|n| snake_case(n)).collect())
                    [*self as usize]
            }
            /// Parse a snake_case name.
            pub fn parse(s: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|v| v.as_str() == s)
            }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
        impl ToJson for $name {
            fn write_json(&self, out: &mut String) {
                self.as_str().write_json(out);
            }
        }
    };
}

str_enum!(
    /// Marketing channel. GA4 default channel group (support.google.com/analytics/answer/9756891)
    /// plus `internal`, `dark_social`, `bot`, `unknown`.
    Channel { Direct, OrganicSearch, PaidSearch, OrganicSocial, PaidSocial, OrganicVideo, PaidVideo,
    OrganicShopping, PaidShopping, Display, CrossNetwork, PaidOther, AiAssistant, PaidAi, Email,
    Affiliate, Referral, Audio, Sms, MobilePush, Internal, DarkSocial, Bot, Unknown });

impl Channel {
    /// Paid / organic split for this channel.
    pub fn paid(&self) -> Paid {
        use Channel::*;
        match self {
            PaidSearch | PaidSocial | PaidVideo | PaidShopping | Display | CrossNetwork
            | PaidOther | PaidAi | Affiliate => Paid::Paid,
            Bot | Unknown | DarkSocial => Paid::Unknown,
            _ => Paid::Organic,
        }
    }
}

str_enum!(/** Paid vs organic. */ Paid { Paid, Organic, Unknown });
str_enum!(
    /// What kind of interaction produced the request.
    Interaction { Click, Share, AdClick, Navigation, Internal, Prefetch, PreviewUnfurl, AiUserFetch, Crawl, AdVerification, Monitoring, Automation, Unknown });
str_enum!(
    /// Where the page is rendered.
    Surface { StandaloneBrowser, WebviewIab, SafariViewController, CustomTab, Pwa, Twa, AppClip, InstantApp,
    AmpCache, EmailClient, Webmail, NativeApp, Bot, Unknown });
str_enum!(
    /// Operating system / platform.
    Os { Ios, Ipados, Android, Windows, Macos, Linux, Chromeos, Harmonyos, Kaios, Tvos, Tizen, Webos, FireOs,
    Roku, AndroidTv, CastOs, Playstation, Xbox, NintendoSwitch, Steamos, MetaHorizonOs, Visionos, PicoOs,
    WearOs, Watchos, AndroidAutomotive, Tesla, Fuchsia, Unknown });
str_enum!(
    /// Device form factor (Sec-CH-UA-Form-Factors vocabulary + extras).
    FormFactor { Phone, Tablet, Desktop, Tv, Console, XrHeadset, Wearable, Car, EReader, SmartDisplay, Embedded, Unknown });
str_enum!(
    /// Bot class. `ai_user_fetch` is a human proxy (count separately), the rest are excluded.
    BotClass { SearchCrawler, AiCrawler, AiSearchCrawler, AiUserFetch, AdVerifier, LinkPreview, SeoTool, Monitoring, Prefetch, Automation });
str_enum!(
    /// Kind of signal behind an [`Evidence`].
    SignalKind { Utm, ClickId, WeakClickId, UaToken, XRequestedWith, ClientHint, ShareMarker, AndroidAppReferrer,
    Referrer, SearchKeyword, AiPathReferrer, ExternalList, OssList, SecFetch, NoReferrer, BotUa, BotHeader, Prefetch, Internal });

// ---------------------------------------------------------------- public I/O types

/// One request. Header names are matched case-insensitively.
#[derive(Debug, Clone, Default)]
pub struct Request {
    /// Absolute URL, or path (then `host` header / `self_hosts[0]` is used).
    pub url: String,
    pub referer: Option<String>,
    pub user_agent: Option<String>,
    pub headers: BTreeMap<String, String>,
    /// Your own hostnames (subdomains included). Defaults to the URL host.
    pub self_hosts: Vec<String>,
    /// Your own Android package names (TWA detection).
    pub self_android_packages: Vec<String>,
}

/// One signal with its documented source.
#[derive(Debug, Clone, PartialEq)]
pub struct Evidence {
    pub kind: SignalKind,
    /// e.g. `click_id:gclid`, `ua_token:instagram`.
    pub signal: String,
    /// Likelihood weight in 0..1 used in the noisy-OR.
    pub weight: f64,
    pub channel: Option<Channel>,
    pub source: Option<String>,
    /// Documentation URL of the rule.
    pub rule_src: Option<String>,
    /// Date the rule was verified against `rule_src` (None = unverified).
    pub verified: Option<String>,
}

/// A ranked attribution hypothesis.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub channel: Channel,
    pub source: String,
    pub medium: String,
    pub campaign: Option<String>,
    pub paid: Paid,
    pub interaction: Interaction,
    /// Noisy-OR belief 0..=1.
    pub confidence: f64,
    /// confidence / Σ confidence over candidates.
    pub share: f64,
    /// Indices into `Attribution.evidence`.
    pub evidence: Vec<usize>,
}

/// Client taxonomy.
#[derive(Debug, Clone)]
pub struct Client {
    pub os: Os,
    pub os_version: Option<String>,
    /// UA-reduction froze the version (macOS 10_15_7, Android "K" model).
    pub os_version_frozen: bool,
    pub form_factor: FormFactor,
    pub device: Option<String>,
    pub browser: Option<String>,
    pub surface: Surface,
    /// Host app for in-app browsers / referring app.
    pub app: Option<String>,
    pub client_hints_used: bool,
    pub confidence: f64,
    /// Documentation of the device rule that matched.
    pub rule_src: Option<String>,
}

/// Bot verdict.
#[derive(Debug, Clone)]
pub struct Bot {
    pub class: BotClass,
    pub token: String,
    pub operator: String,
    /// True if the request should be excluded from human analytics.
    pub exclude: bool,
    pub rule_src: Option<String>,
}

/// Full result.
#[derive(Debug, Clone)]
pub struct Attribution {
    /// All candidates, highest confidence first. Never empty.
    pub candidates: Vec<Candidate>,
    pub client: Client,
    pub bot: Option<Bot>,
    pub is_prefetch: bool,
    /// Starts a new session/touch (false for internal, bots, prefetch).
    pub new_touch: bool,
    pub evidence: Vec<Evidence>,
}

impl Attribution {
    /// Highest-ranked candidate.
    pub fn top(&self) -> &Candidate {
        &self.candidates[0]
    }
}

// ---------------------------------------------------------------- JSON

/// A [`Request`] that could not be read from JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestError(pub String);
impl std::fmt::Display for RequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid request: {}", self.0)
    }
}
impl std::error::Error for RequestError {}

impl Request {
    /// Read a request object: `url` is required; `referer` and `user_agent` may be strings or
    /// null; `headers` is an object of strings; `self_hosts` and `self_android_packages` are
    /// arrays of strings. Unknown members are ignored.
    pub fn from_json(text: &str) -> Result<Request, RequestError> {
        let v = Value::parse(text).map_err(|e| RequestError(e.to_string()))?;
        Self::from_owned_json(v)
    }

    /// [`Self::from_json`] for an already parsed value.
    pub fn from_json_value(v: &Value) -> Result<Request, RequestError> {
        Self::from_owned_json(v.clone())
    }

    fn from_owned_json(v: Value) -> Result<Request, RequestError> {
        const KEYS: [&str; 6] = [
            "url",
            "referer",
            "user_agent",
            "headers",
            "self_hosts",
            "self_android_packages",
        ];
        let bad = |k: &str, want: &str| RequestError(format!("`{k}` must be {want}"));
        let Value::Object(members) = v else {
            return Err(RequestError("expected a JSON object".into()));
        };
        // The last occurrence of a duplicated member wins, as for `Value` lookups.
        let mut slots: [Value; 6] = Default::default();
        for (k, x) in members {
            if let Some(i) = KEYS.iter().position(|&want| want == k) {
                slots[i] = x;
            }
        }
        let [url, referer, user_agent, headers, self_hosts, self_android_packages] = slots;
        let opt_str = |k: &str, x: Value| match x {
            Value::Null => Ok(None),
            Value::String(s) => Ok(Some(s)),
            _ => Err(bad(k, "a string or null")),
        };
        let strings = |k: &str, x: Value| match x {
            Value::Null => Ok(Vec::new()),
            Value::Array(a) => a
                .into_iter()
                .map(|x| match x {
                    Value::String(s) => Ok(s),
                    _ => Err(bad(k, "an array of strings")),
                })
                .collect(),
            _ => Err(bad(k, "an array of strings")),
        };
        let headers = match headers {
            Value::Null => BTreeMap::new(),
            Value::Object(m) => m
                .into_iter()
                .map(|(k, x)| match x {
                    Value::String(s) => Ok((k, s)),
                    _ => Err(bad("headers", "an object of strings")),
                })
                .collect::<Result<_, _>>()?,
            _ => return Err(bad("headers", "an object of strings")),
        };
        Ok(Request {
            url: opt_str("url", url)?.ok_or_else(|| bad("url", "a string"))?,
            referer: opt_str("referer", referer)?,
            user_agent: opt_str("user_agent", user_agent)?,
            headers,
            self_hosts: strings("self_hosts", self_hosts)?,
            self_android_packages: strings("self_android_packages", self_android_packages)?,
        })
    }
}

impl ToJson for Request {
    fn write_json(&self, out: &mut String) {
        ObjectWriter::new(out)
            .field("url", &self.url)
            .field("referer", &self.referer)
            .field("user_agent", &self.user_agent)
            .field("headers", &HeaderMap(&self.headers))
            .field("self_hosts", &self.self_hosts)
            .field("self_android_packages", &self.self_android_packages)
            .finish();
    }
}

struct HeaderMap<'a>(&'a BTreeMap<String, String>);

impl ToJson for HeaderMap<'_> {
    fn write_json(&self, out: &mut String) {
        let mut o = ObjectWriter::new(out);
        for (k, v) in self.0 {
            o = o.field(k, v);
        }
        o.finish();
    }
}

impl ToJson for Evidence {
    fn write_json(&self, out: &mut String) {
        ObjectWriter::new(out)
            .field("kind", &self.kind)
            .field("signal", &self.signal)
            .field("weight", &self.weight)
            .opt("channel", self.channel.as_ref())
            .opt("source", self.source.as_ref())
            .opt("rule_src", self.rule_src.as_ref())
            .opt("verified", self.verified.as_ref())
            .finish();
    }
}

impl ToJson for Candidate {
    fn write_json(&self, out: &mut String) {
        ObjectWriter::new(out)
            .field("channel", &self.channel)
            .field("source", &self.source)
            .field("medium", &self.medium)
            .opt("campaign", self.campaign.as_ref())
            .field("paid", &self.paid)
            .field("interaction", &self.interaction)
            .field("confidence", &self.confidence)
            .field("share", &self.share)
            .field("evidence", &self.evidence)
            .finish();
    }
}

impl ToJson for Client {
    fn write_json(&self, out: &mut String) {
        ObjectWriter::new(out)
            .field("os", &self.os)
            .opt("os_version", self.os_version.as_ref())
            .field("os_version_frozen", &self.os_version_frozen)
            .field("form_factor", &self.form_factor)
            .opt("device", self.device.as_ref())
            .opt("browser", self.browser.as_ref())
            .field("surface", &self.surface)
            .opt("app", self.app.as_ref())
            .field("client_hints_used", &self.client_hints_used)
            .field("confidence", &self.confidence)
            .opt("rule_src", self.rule_src.as_ref())
            .finish();
    }
}

impl ToJson for Bot {
    fn write_json(&self, out: &mut String) {
        ObjectWriter::new(out)
            .field("class", &self.class)
            .field("token", &self.token)
            .field("operator", &self.operator)
            .field("exclude", &self.exclude)
            .opt("rule_src", self.rule_src.as_ref())
            .finish();
    }
}

impl ToJson for Attribution {
    fn write_json(&self, out: &mut String) {
        ObjectWriter::new(out)
            .field("candidates", &self.candidates)
            .field("client", &self.client)
            .opt("bot", self.bot.as_ref())
            .field("is_prefetch", &self.is_prefetch)
            .field("new_touch", &self.new_touch)
            .field("evidence", &self.evidence)
            .finish();
    }
}

/// Error from loading rules.
#[derive(Debug)]
pub struct RuleError(pub String);
impl std::fmt::Display for RuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "rule error: {}", self.0)
    }
}
impl std::error::Error for RuleError {}

// ---------------------------------------------------------------- weights (documented in docs/API.md)

/// Likelihood weights per signal kind.
pub mod weights {
    pub const UTM_FULL: f64 = 0.95;
    pub const UTM_PARTIAL: f64 = 0.80;
    pub const CLICK_ID: f64 = 0.93;
    pub const WEAK_CLICK_ID: f64 = 0.70;
    pub const UA_TOKEN: f64 = 0.85;
    /// Raised from 0.75 (v2): WebView X-Requested-With removal was cancelled (androidx.webkit 1.15.0).
    pub const X_REQUESTED_WITH: f64 = 0.85;
    pub const SHARE_MARKER: f64 = 0.70;
    pub const ANDROID_APP_REF: f64 = 0.80;
    pub const REFERRER: f64 = 0.75;
    /// The referrer carries the engine's documented search-keyword parameter, so it is a results
    /// page rather than the engine's home page. Source-only: it does not imply paid or organic.
    pub const SEARCH_KEYWORD: f64 = 0.70;
    pub const AI_PATH: f64 = 0.80;
    pub const OSS_LIST: f64 = 0.65;
    pub const EXTERNAL_LIST: f64 = 0.60;
    pub const UNKNOWN_REFERRER: f64 = 0.60;
    pub const INTERNAL: f64 = 0.90;
    pub const DIRECT: f64 = 0.50;
    pub const DARK_SOCIAL_SECFETCH: f64 = 0.45;
    pub const DARK_SOCIAL_WEBVIEW: f64 = 0.35;
    pub const BOT_DOCUMENTED: f64 = 0.97;
    pub const BOT_GENERIC: f64 = 0.85;
    pub const FAMILY_FACTOR: f64 = 0.80;
    /// Discount applied to a hypothesis contradicted by explicit tagging for the same source.
    pub const CONFLICT_FACTOR: f64 = 0.50;
}

// ---------------------------------------------------------------- rules

#[derive(Debug, Clone)]
struct ClickIdRule {
    param: String,
    source: String,
    medium: String,
    channel: Channel,
    weak: bool,
    src: Option<String>,
    verified: Option<String>,
}
#[derive(Debug, Clone)]
struct UaTokenRule {
    source: String,
    kind: String,
    src: Option<String>,
    verified: Option<String>,
}
#[derive(Debug, Clone)]
struct RefRule {
    source: String,
    kind: String,
    paid: bool,
    src: Option<String>,
    verified: Option<String>,
}
#[derive(Debug, Clone)]
struct BotRule {
    class: BotClass,
    token: String,
    operator: String,
    src: Option<String>,
    verified: Option<String>,
}
#[derive(Debug, Clone)]
struct BotHeaderRule {
    header: String,
    /// Lowercase; compared against the lowercased header value.
    contains: String,
    class: BotClass,
    operator: String,
    src: Option<String>,
}
#[derive(Debug, Clone)]
struct DeviceRule {
    os: Os,
    form_factor: Option<FormFactor>,
    device: Option<String>,
    src: Option<String>,
    verified: Option<String>,
}
/// A share marker with its patterns compiled once at load time.
#[derive(Debug)]
struct ShareMarker {
    param: String,
    source: String,
    /// Only fires when the value matches.
    value_rx: Option<Regex>,
    /// Only fires when the referrer host matches.
    ref_host_rx: Option<Regex>,
    /// Only fires when this other parameter is also present.
    with_param: Option<String>,
    src: Option<String>,
    verified: Option<String>,
}

#[derive(Debug)]
struct Compiled<T> {
    rx: Regex,
    r: T,
}

trait HasPattern {
    fn rx(&self) -> &Regex;
}
impl<T> HasPattern for Compiled<T> {
    fn rx(&self) -> &Regex {
        &self.rx
    }
}
impl HasPattern for (Regex, String) {
    fn rx(&self) -> &Regex {
        &self.0
    }
}

/// Rules tried in order against one string, after a single-pass [`Screen`] over all of them.
#[derive(Debug)]
struct RuleList<T> {
    items: Vec<T>,
    screen: Screen,
}

impl<T: HasPattern> RuleList<T> {
    fn new(items: Vec<T>) -> Self {
        let screen = Screen::new(items.iter().map(HasPattern::rx));
        RuleList { items, screen }
    }
    /// The rules whose pattern matches `h`, in list order.
    fn matching<'a>(&'a self, h: &'a Haystack<'a>) -> impl Iterator<Item = &'a T> + 'a {
        self.screen
            .candidates(h)
            .into_indices()
            .map(|i| &self.items[i])
            .filter(move |t| t.rx().is_match_in(h))
    }
    fn first(&self, s: &str) -> Option<&T> {
        let h = Haystack::new(s);
        let mut hits = self.screen.candidates(&h).into_indices();
        hits.find_map(|i| Some(&self.items[i]).filter(|t| t.rx().is_match_in(&h)))
    }
}

/// Source categories a `type` field may name.
const KINDS: [&str; 8] = [
    "search", "social", "video", "email", "ai", "display", "shopping", "referral",
];

/// One object of a rule file, with its location for error messages.
struct Rec<'a> {
    v: &'a Value,
    at: String,
}

impl<'a> Rec<'a> {
    fn new(v: &'a Value, at: impl Into<String>) -> Result<Self, RuleError> {
        let at = at.into();
        if v.as_object().is_none() {
            return Err(RuleError(format!("{at}: expected an object")));
        }
        Ok(Rec { v, at })
    }
    fn err(&self, k: &str, what: impl std::fmt::Display) -> RuleError {
        RuleError(format!("{}.{k}: {what}", self.at))
    }
    fn str(&self, k: &str) -> Result<String, RuleError> {
        self.v[k]
            .as_str()
            .map(String::from)
            .ok_or_else(|| self.err(k, "expected a string"))
    }
    /// Absent and `null` are both `None`.
    fn opt(&self, k: &str) -> Result<Option<String>, RuleError> {
        match &self.v[k] {
            Value::Null => Ok(None),
            Value::String(s) => Ok(Some(s.clone())),
            _ => Err(self.err(k, "expected a string or null")),
        }
    }
    fn flag(&self, k: &str) -> Result<bool, RuleError> {
        match &self.v[k] {
            Value::Null => Ok(false),
            Value::Bool(b) => Ok(*b),
            _ => Err(self.err(k, "expected a boolean")),
        }
    }
    fn regex(&self, k: &str, ci: bool) -> Result<Regex, RuleError> {
        Regex::build(&self.str(k)?, ci).map_err(|e| self.err(k, e))
    }
    fn opt_regex(&self, k: &str, ci: bool) -> Result<Option<Regex>, RuleError> {
        self.opt(k)?
            .map(|p| Regex::build(&p, ci).map_err(|e| self.err(k, e)))
            .transpose()
    }
    fn parsed<T>(&self, k: &str, parse: impl Fn(&str) -> Option<T>) -> Result<T, RuleError> {
        let s = self.str(k)?;
        parse(&s).ok_or_else(|| self.err(k, format!("unknown {k} {s:?}")))
    }
    fn kind(&self) -> Result<String, RuleError> {
        self.parsed("type", |s| KINDS.contains(&s).then(|| s.to_string()))
    }
    fn src(&self) -> Result<(Option<String>, Option<String>), RuleError> {
        Ok((self.opt("src")?, self.opt("verified")?))
    }
}

/// The objects of the array `v[key]`; an absent array is empty only when `optional`.
fn records<'a>(v: &'a Value, key: &str, optional: bool) -> Result<Vec<Rec<'a>>, RuleError> {
    match &v[key] {
        Value::Null if optional => Ok(Vec::new()),
        Value::Array(a) => a
            .iter()
            .enumerate()
            .map(|(i, x)| Rec::new(x, format!("{key}[{i}]")))
            .collect(),
        _ => Err(RuleError(format!("{key}: expected an array"))),
    }
}

fn string_map(v: &Value, at: &str) -> Result<HashMap<String, String>, RuleError> {
    v.as_object()
        .ok_or_else(|| RuleError(format!("{at}: expected an object")))?
        .iter()
        .map(|(k, x)| match x.as_str() {
            Some(s) => Ok((k.clone(), s.to_string())),
            None => Err(RuleError(format!("{at}.{k}: expected a string"))),
        })
        .collect()
}

fn string_list(v: &Value, at: &str) -> Result<Vec<String>, RuleError> {
    v.as_array()
        .ok_or_else(|| RuleError(format!("{at}: expected an array")))?
        .iter()
        .enumerate()
        .map(|(i, x)| {
            x.as_str()
                .map(String::from)
                .ok_or_else(|| RuleError(format!("{at}[{i}]: expected a string")))
        })
        .collect()
}

fn parse_json(text: &str, what: &str) -> Result<Value, RuleError> {
    Value::parse(text).map_err(|e| RuleError(format!("{what}: {e}")))
}

/// Regex-escape a literal.
fn escape(lit: &str) -> String {
    let mut out = String::with_capacity(lit.len() + 4);
    for c in lit.chars() {
        if "\\.+*?()|[]{}^$#&-~".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Key of the query parameter named `param`, compared case-insensitively, as it appeared in the
/// URL.
fn qkey_ci<'a>(q: &'a [(String, String)], param: &str) -> Option<&'a str> {
    q.iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(param))
        .map(|(k, _)| k.as_str())
}

/// First value that appears twice ignoring case, if any. Used to reject rule sets whose entries
/// would both match the same input and be counted twice.
fn first_ci_duplicate<'a>(it: impl Iterator<Item = &'a str>) -> Option<String> {
    let mut seen = HashSet::new();
    for v in it {
        if !seen.insert(v.to_ascii_lowercase()) {
            return Some(v.to_string());
        }
    }
    None
}

/// External referrer entry (e.g. GPL-licensed lists you load yourself; never shipped).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalReferrer {
    pub source: String,
    pub medium: Option<String>,
}

/// The attribution engine. Construct once, reuse (it is `Send + Sync`).
#[derive(Debug)]
pub struct Attributor {
    click_ids: Vec<ClickIdRule>,
    ua_tokens: RuleList<Compiled<UaTokenRule>>,
    xrw_apps: HashMap<String, String>,
    xrw_src: String,
    referrers: RuleList<Compiled<RefRule>>,
    ai_paths: Vec<(String, Regex, String)>,
    share_markers: Vec<ShareMarker>,
    mastodon: HashSet<String>,
    bots: RuleList<Compiled<BotRule>>,
    bot_headers: Vec<BotHeaderRule>,
    devices: RuleList<Compiled<DeviceRule>>,
    browsers: RuleList<(Regex, String)>,
    ch_platform: HashMap<String, String>,
    ch_form: HashMap<String, String>,
    ga4: Ga4,
    oss: HashMap<String, (String, String, bool)>,
    keyword_params: HashMap<String, Vec<String>>,
    external: HashMap<String, ExternalReferrer>,
    ga4_src: String,
}

#[derive(Debug)]
struct Ga4 {
    paid_medium: Regex,
    shop_camp: Regex,
    display: Regex,
    social_medium: Regex,
    paid_source: Regex,
    shop_host: Regex,
    video_host: Regex,
}

impl Default for Attributor {
    fn default() -> Self {
        Self::new()
    }
}

impl Attributor {
    /// Engine with embedded rules. Panics only if the embedded data is corrupt (covered by tests).
    pub fn new() -> Self {
        Self::from_json(RULES, DEVICES, OSS).expect("embedded rules are valid")
    }

    /// Build from rule JSON strings (same schema as `rules/*.json`).
    ///
    /// Every field is checked: a missing or mistyped field, an unknown enum name or `type`, a
    /// pattern that does not compile, or two rules that would match the same input is an error
    /// naming the offending entry, never a silent default.
    pub fn from_json(rules: &str, devices: &str, oss: &str) -> Result<Self, RuleError> {
        let r = parse_json(rules, "rules")?;
        let d = parse_json(devices, "devices")?;
        let o = parse_json(oss, "oss")?;

        let click_ids = records(&r, "clickIds", false)?
            .iter()
            .map(|x| {
                let (src, verified) = x.src()?;
                Ok(ClickIdRule {
                    param: x.str("param")?,
                    source: x.str("source")?,
                    medium: x.str("medium")?,
                    channel: x.parsed("channel", Channel::parse)?,
                    weak: x.flag("weak")?,
                    src,
                    verified,
                })
            })
            .collect::<Result<Vec<_>, RuleError>>()?;
        // Parameters are matched case-insensitively, so two rules differing only in case would
        // both fire on one parameter and double-count it in the noisy-OR.
        if let Some(d) = first_ci_duplicate(click_ids.iter().map(|c| c.param.as_str())) {
            return Err(RuleError(format!(
                "clickIds: duplicate parameter (case-insensitive): {d}"
            )));
        }
        let ua_tokens = records(&r, "uaTokens", false)?
            .iter()
            .map(|x| {
                let (src, verified) = x.src()?;
                Ok(Compiled {
                    rx: x.regex("re", true)?,
                    r: UaTokenRule {
                        source: x.str("source")?,
                        kind: x.kind()?,
                        src,
                        verified,
                    },
                })
            })
            .collect::<Result<Vec<_>, RuleError>>()?;
        let xrw = Rec::new(&r["xRequestedWith"], "xRequestedWith")?;
        let xrw_apps = string_map(&xrw.v["apps"], "xRequestedWith.apps")?;
        let referrers = records(&r, "referrers", false)?
            .iter()
            .map(|x| {
                let (src, verified) = x.src()?;
                Ok(Compiled {
                    rx: x.regex("re", true)?,
                    r: RefRule {
                        source: x.str("source")?,
                        kind: x.kind()?,
                        paid: x.flag("paid")?,
                        src,
                        verified,
                    },
                })
            })
            .collect::<Result<Vec<_>, RuleError>>()?;
        let ai_paths = records(&r, "aiPathReferrers", true)?
            .iter()
            .map(|x| Ok((x.str("host")?, x.regex("path", false)?, x.str("source")?)))
            .collect::<Result<Vec<_>, RuleError>>()?;
        let share_markers = records(&r, "shareMarkers", false)?
            .iter()
            .map(|x| {
                let (src, verified) = x.src()?;
                Ok(ShareMarker {
                    param: x.str("param")?,
                    source: x.str("source")?,
                    value_rx: x.opt_regex("valueRe", false)?,
                    ref_host_rx: x.opt_regex("refHost", true)?,
                    with_param: x.opt("withParam")?,
                    src,
                    verified,
                })
            })
            .collect::<Result<Vec<_>, RuleError>>()?;
        let mastodon = match &r["mastodonInstances"] {
            Value::Null => HashSet::new(),
            v => string_list(v, "mastodonInstances")?.into_iter().collect(),
        };

        let bots = records(&r, "botRules", false)?
            .iter()
            .map(|x| {
                let (src, verified) = x.src()?;
                let token = x.str("token")?;
                // A boundary on each side, except after a token that ends in punctuation (e.g.
                // `Mozilla/5.0 (compatible; Foo`), where `\b` would demand a word character next.
                let tail = if token.ends_with(|c: char| c.is_alphanumeric()) {
                    "\\b"
                } else {
                    ""
                };
                let rx = Regex::case_insensitive(&format!("\\b{}{tail}", escape(&token)))
                    .map_err(|e| x.err("token", e))?;
                Ok(Compiled {
                    rx,
                    r: BotRule {
                        class: x.parsed("class", BotClass::parse)?,
                        token,
                        operator: x.str("operator")?,
                        src,
                        verified,
                    },
                })
            })
            .collect::<Result<Vec<_>, RuleError>>()?;
        // Tokens are matched case-insensitively. Two rules differing only in case both match, and
        // the winner was decided by JSON order — which silently flipped `exclude` when the two
        // disagreed about the class.
        if let Some(d) = first_ci_duplicate(bots.iter().map(|b| b.r.token.as_str())) {
            return Err(RuleError(format!(
                "botRules: duplicate token (case-insensitive): {d}"
            )));
        }
        let bot_headers = records(&r, "botHeaders", false)?
            .iter()
            .map(|x| {
                Ok(BotHeaderRule {
                    header: x.str("header")?.to_ascii_lowercase(),
                    contains: x.str("contains")?.to_ascii_lowercase(),
                    class: x.parsed("class", BotClass::parse)?,
                    operator: x.str("operator")?,
                    src: x.opt("src")?,
                })
            })
            .collect::<Result<Vec<_>, RuleError>>()?;

        let g = Rec::new(&r["ga4"], "ga4")?;
        let ga4 = Ga4 {
            paid_medium: g.regex("paidMedium", false)?,
            shop_camp: g.regex("shoppingCampaign", false)?,
            display: g.regex("display", false)?,
            social_medium: g.regex("socialMedium", false)?,
            paid_source: g.regex("paidSource", true)?,
            shop_host: g.regex("shoppingHosts", true)?,
            video_host: g.regex("videoHosts", true)?,
        };

        let devices = records(&d, "devices", false)?
            .iter()
            .map(|x| {
                let (src, verified) = x.src()?;
                let form_factor = match x.opt("form_factor")? {
                    None => None,
                    Some(_) => Some(x.parsed("form_factor", FormFactor::parse)?),
                };
                Ok(Compiled {
                    rx: x.regex("re", false)?,
                    r: DeviceRule {
                        os: x.parsed("os", Os::parse)?,
                        form_factor,
                        device: x.opt("device")?,
                        src,
                        verified,
                    },
                })
            })
            .collect::<Result<Vec<_>, RuleError>>()?;
        let browsers = records(&d, "browsers", true)?
            .iter()
            .map(|x| Ok((x.regex("re", false)?, x.str("name")?)))
            .collect::<Result<Vec<_>, RuleError>>()?;
        let ch = Rec::new(&d["clientHints"], "clientHints")?;
        let ch_platform = string_map(&ch.v["platform_map"], "clientHints.platform_map")?;
        let ch_form = string_map(&ch.v["form_factor_map"], "clientHints.form_factor_map")?;
        for (k, os) in &ch_platform {
            if Os::parse(os).is_none() {
                return Err(RuleError(format!(
                    "clientHints.platform_map.{k}: unknown os {os:?}"
                )));
            }
        }
        for (k, ff) in &ch_form {
            if FormFactor::parse(ff).is_none() {
                return Err(RuleError(format!(
                    "clientHints.form_factor_map.{k}: unknown form_factor {ff:?}"
                )));
            }
        }

        let hosts = o["hosts"]
            .as_object()
            .ok_or_else(|| RuleError("hosts: expected an object".into()))?;
        let mut oss = HashMap::with_capacity(hosts.len());
        for (k, v) in hosts {
            let x = Rec::new(v, format!("hosts.{k}"))?;
            oss.insert(k.clone(), (x.str("name")?, x.kind()?, x.flag("prefix")?));
        }
        let keyword_params = match o["keywordParams"].as_object() {
            None if o["keywordParams"].is_null() => HashMap::new(),
            None => return Err(RuleError("keywordParams: expected an object".into())),
            Some(m) => m
                .iter()
                .map(|(k, v)| Ok((k.clone(), string_list(v, &format!("keywordParams.{k}"))?)))
                .collect::<Result<_, RuleError>>()?,
        };

        Ok(Self {
            click_ids,
            ua_tokens: RuleList::new(ua_tokens),
            xrw_apps,
            xrw_src: xrw.str("_src")?,
            referrers: RuleList::new(referrers),
            ai_paths,
            share_markers,
            mastodon,
            bots: RuleList::new(bots),
            bot_headers,
            devices: RuleList::new(devices),
            browsers: RuleList::new(browsers),
            ch_platform,
            ch_form,
            ga4,
            oss,
            keyword_params,
            external: HashMap::new(),
            ga4_src: g.str("_src")?,
        })
    }

    /// Add an external referrer list `{host: {source, medium}}` or `{"referrers": {...}}`.
    /// Use this for GPL/AGPL/LGPL-derived lists, which this crate does not ship.
    pub fn with_external_referrers(mut self, json: &str) -> Result<Self, RuleError> {
        let v = parse_json(json, "external referrers")?;
        let m = match v.get("referrers") {
            Some(inner) => inner,
            None => &v,
        };
        let entries = m
            .as_object()
            .ok_or_else(|| RuleError("external referrers: expected an object".into()))?;
        for (host, e) in entries {
            let x = Rec::new(e, host.as_str())?;
            self.external.insert(
                host.clone(),
                ExternalReferrer {
                    source: x.str("source")?,
                    medium: x.opt("medium")?,
                },
            );
        }
        Ok(self)
    }

    /// Load [`Self::with_external_referrers`] from a file path.
    pub fn with_external_file(self, path: &str) -> Result<Self, RuleError> {
        let t = std::fs::read_to_string(path).map_err(|x| RuleError(format!("{path}: {x}")))?;
        self.with_external_referrers(&t)
    }

    // ------------------------------------------------------------ client taxonomy

    /// Classify OS, form factor, browser, surface and in-app host from UA + client hints.
    pub fn classify_client(&self, req: &Request) -> Client {
        let h = Headers::new(req);
        let ua = h.ua();
        let mut c = Client {
            os: Os::Unknown,
            os_version: None,
            os_version_frozen: false,
            form_factor: FormFactor::Unknown,
            device: None,
            browser: None,
            surface: Surface::Unknown,
            app: None,
            client_hints_used: false,
            confidence: 0.5,
            rule_src: None,
        };
        let hua = Haystack::new(ua);
        if let Some(d) = self.devices.matching(&hua).next() {
            c.os = d.r.os;
            c.form_factor = d.r.form_factor.unwrap_or(FormFactor::Unknown);
            c.device = d.r.device.clone();
            c.rule_src = d.r.src.clone();
            c.confidence = if d.r.verified.is_some() { 0.85 } else { 0.6 };
        }
        // OS version + UA-reduction freeze (chromium.org/updates/ua-reduction)
        let ver = |r: &Regex| r.captures(ua).map(|m| m[1].replace('_', "."));
        c.os_version = match c.os {
            Os::Ios | Os::Ipados => ver(re!(r"OS (\d+[_.]\d+(?:[_.]\d+)?)")),
            Os::Android | Os::AndroidTv | Os::FireOs => ver(re!(r"Android (\d+(?:\.\d+)*)")),
            Os::Macos => ver(re!(r"Mac OS X (\d+[_.]\d+(?:[_.]\d+)?)")),
            Os::Windows => ver(re!(r"Windows NT (\d+\.\d+)")),
            Os::Harmonyos => ver(re!(r"(?:OpenHarmony|HarmonyOS) (\d+(?:\.\d+)*)")),
            Os::Kaios => ver(re!(r"KAIOS/(\d+(?:\.\d+)*)")),
            Os::Tizen => ver(re!(r"Tizen (\d+(?:\.\d+)*)")),
            Os::Playstation => ver(re!(r"PlayStation \d/(\d+(?:\.\d+)*)")),
            _ => None,
        };
        c.os_version_frozen =
            ua.contains("Mac OS X 10_15_7") || re!(r"Android 10; K\)").is_match(ua);
        // HarmonyOS NEXT ArkWeb: "({Phone|Tablet|PC}; OpenHarmony x)"
        if let Some(m) = re!(r"\((Phone|Tablet|PC); OpenHarmony").captures(ua) {
            c.form_factor = match &m[1] {
                "Phone" => FormFactor::Phone,
                "Tablet" => FormFactor::Tablet,
                _ => FormFactor::Desktop,
            };
        }
        if c.os == Os::Harmonyos && c.form_factor == FormFactor::Unknown && ua.contains("Mobile") {
            c.form_factor = FormFactor::Phone;
        }
        // Client hints override UA (MDN Client hints; WICG UA-CH)
        let unq = |x: &str| x.trim().trim_matches('"').to_string();
        if let Some(p) = h.get("sec-ch-ua-platform") {
            if let Some(os) = self.ch_platform.get(&unq(p)).and_then(|o| Os::parse(o)) {
                // keep specific platforms (TV/console/XR) found in the UA
                if matches!(
                    c.os,
                    Os::Unknown
                        | Os::Linux
                        | Os::Macos
                        | Os::Windows
                        | Os::Android
                        | Os::Ios
                        | Os::Chromeos
                ) || os == Os::Android && c.os == Os::Harmonyos
                {
                    c.os = os;
                }
                c.client_hints_used = true;
            }
        }
        if let Some(v) = h.get("sec-ch-ua-platform-version") {
            let v = unq(v);
            c.os_version = Some(if c.os == Os::Windows {
                // learn.microsoft.com/en-us/microsoft-edge/web-platform/how-to-detect-win11
                let major: u32 = v
                    .split('.')
                    .next()
                    .and_then(|x| x.parse().ok())
                    .unwrap_or(0);
                if major >= 13 {
                    format!("11 ({v})")
                } else if major > 0 {
                    format!("10 ({v})")
                } else {
                    v
                }
            } else {
                v
            });
            c.os_version_frozen = false;
            c.client_hints_used = true;
        }
        if let Some(m) = h.get("sec-ch-ua-mobile") {
            if m.trim() == "?1"
                && matches!(
                    c.form_factor,
                    FormFactor::Unknown | FormFactor::Desktop | FormFactor::Tablet
                )
            {
                c.form_factor = FormFactor::Phone;
            }
            c.client_hints_used = true;
        }
        if let Some(ff) = h.get("sec-ch-ua-form-factors") {
            let first = ff
                .split(',')
                .map(unq)
                .find_map(|x| self.ch_form.get(&x).cloned());
            if let Some(f) = first.and_then(|f| FormFactor::parse(&f)) {
                c.form_factor = f;
                c.client_hints_used = true;
            }
        }
        if let Some(m) = h.get("sec-ch-ua-model") {
            let m = unq(m);
            if !m.is_empty() {
                c.device = Some(m);
                c.client_hints_used = true;
            }
        }
        if c.client_hints_used {
            c.confidence = (c.confidence + 0.1).min(0.95);
        }
        c.browser = self.browsers.matching(&hua).next().map(|(_, n)| n.clone());
        // surface
        let iab = self.ua_tokens.matching(&hua).next();
        let xrw = h.get("x-requested-with").filter(|x| !is_browser_pkg(x));
        let (ascheme, ahost, _) = parse_ref(h.referer());
        c.surface = if let Some(t) = iab {
            c.app = Some(t.r.source.clone());
            Surface::WebviewIab
        } else if let Some(x) = xrw {
            c.app = Some(self.xrw_apps.get(x).cloned().unwrap_or_else(|| x.to_string()));
            if is_mail_pkg(x) { Surface::EmailClient } else { Surface::WebviewIab }
        } else if ua.contains("; wv)") {
            Surface::WebviewIab
        } else if ascheme == "android-app" {
            c.app = Some(self.xrw_apps.get(&ahost).cloned().unwrap_or_else(|| ahost.clone()));
            if req.self_android_packages.iter().any(|p| p == &ahost) { Surface::Twa } else { Surface::CustomTab }
        } else if ahost.ends_with(".cdn.ampproject.org") || ahost.ends_with("ampproject.net") {
            Surface::AmpCache
        } else if re!(r"(?i)Microsoft Outlook|Thunderbird/|Lotus-Notes|Airmail").is_match(ua) {
            Surface::EmailClient
        } else if re!(r"^(mail\.google\.com|outlook\.(live|office|office365)\.com|mail\.yahoo\.com|mail\.proton\.me|mail\.aol\.com|mail\.yandex\.\w+|e\.mail\.ru)$").is_match(&ahost) {
            Surface::Webmail
        } else if re!(r"[?&](utm_source|source|src)=(pwa|homescreen|a2hs|web_app_manifest)\b").is_match(&req.url) {
            Surface::Pwa
        } else if ua.is_empty() {
            Surface::Unknown
        } else {
            Surface::StandaloneBrowser
        };
        c
    }

    // ------------------------------------------------------------ bots

    /// Detect bots / fetchers / prefetch.
    pub fn classify_bot(&self, req: &Request) -> Option<Bot> {
        let h = Headers::new(req);
        for b in &self.bot_headers {
            if h.get(&b.header)
                .is_some_and(|v| v.to_ascii_lowercase().contains(&b.contains))
                && b.class != BotClass::Prefetch
            {
                return Some(mk_bot(b.class, &b.header, &b.operator, b.src.clone()));
            }
        }
        let ua = h.ua();
        if ua.is_empty() {
            return Some(mk_bot(
                BotClass::Automation,
                "(empty user-agent)",
                "unknown",
                None,
            ));
        }
        let hua = Haystack::new(ua);
        // prefer documented rules, then the longest token (Applebot-Extended over Applebot);
        // `max_by_key` keeps the last of equal keys, so ties go to the later rule.
        let b = &self
            .bots
            .matching(&hua)
            .max_by_key(|b| (b.r.verified.is_some(), b.r.token.len()))?
            .r;
        Some(mk_bot(b.class, &b.token, &b.operator, b.src.clone()))
    }

    // ------------------------------------------------------------ main entry

    /// Attribute a request. Always returns ≥1 candidate.
    pub fn attribute(&self, req: &Request) -> Attribution {
        let h = Headers::new(req);
        let client = self.classify_client(req);
        let bot = self.classify_bot(req);
        let purpose = ["sec-purpose", "purpose", "x-purpose", "x-moz"]
            .iter()
            .filter_map(|k| h.get(k))
            .collect::<Vec<_>>()
            .join(",")
            .to_ascii_lowercase();
        let is_prefetch = purpose.contains("prefetch") || purpose.contains("prerender");
        let mut ev: Vec<Evidence> = Vec::new();
        let mut meta: HashMap<(Channel, String), (String, Option<String>, Interaction)> =
            HashMap::new();
        let push = |ev: &mut Vec<Evidence>,
                    kind,
                    signal: String,
                    w: f64,
                    ch: Option<Channel>,
                    src: Option<String>,
                    rs: Option<String>,
                    ver: Option<String>| {
            ev.push(Evidence {
                kind,
                signal,
                weight: w,
                channel: ch,
                source: src.map(|x| norm_source(&x)),
                rule_src: rs,
                verified: ver,
            });
        };
        if is_prefetch {
            push(&mut ev, SignalKind::Prefetch, format!("purpose:{purpose}"), 0.0, None, None, Some("https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Sec-Purpose".into()), Some("2026-10-24".into()));
        }

        if let Some(b) = &bot {
            let w = if b.rule_src.is_some() {
                weights::BOT_DOCUMENTED
            } else {
                weights::BOT_GENERIC
            };
            push(
                &mut ev,
                SignalKind::BotUa,
                format!("bot:{}:{}", b.class.as_str(), b.token),
                w,
                Some(Channel::Bot),
                Some(b.operator.clone()),
                b.rule_src.clone(),
                None,
            );
            let inter = match b.class {
                BotClass::AiUserFetch => Interaction::AiUserFetch,
                BotClass::LinkPreview => Interaction::PreviewUnfurl,
                BotClass::Monitoring => Interaction::Monitoring,
                BotClass::AdVerifier => Interaction::AdVerification,
                BotClass::Automation => Interaction::Automation,
                BotClass::Prefetch => Interaction::Prefetch,
                _ => Interaction::Crawl,
            };
            meta.insert(
                (Channel::Bot, norm_source(&b.operator)),
                (b.class.as_str().to_string(), None, inter),
            );
            if b.class != BotClass::AiUserFetch {
                return self.finish(ev, meta, client, bot, is_prefetch, false);
            }
            // AI user fetch: a human asked an assistant -> also an ai_assistant touch
            push(
                &mut ev,
                SignalKind::BotUa,
                format!("user_triggered:{}", b.token),
                0.6,
                Some(Channel::AiAssistant),
                Some(b.operator.clone()),
                b.rule_src.clone(),
                None,
            );
            meta.insert(
                (Channel::AiAssistant, norm_source(&b.operator)),
                ("ai_user_fetch".into(), None, Interaction::AiUserFetch),
            );
            return self.finish(ev, meta, client, bot, is_prefetch, false);
        }

        let url = parse_url(req, &h);
        let q: Vec<(String, String)> = url.as_ref().map(|u| u.query_pairs()).unwrap_or_default();
        let qget = |k: &str| q.iter().find(|(a, _)| a == k).map(|(_, b)| b.as_str());
        let qget_ci = |k: &str| {
            q.iter()
                .find(|(a, _)| a.eq_ignore_ascii_case(k))
                .map(|(_, b)| b.as_str())
        };
        let self_hosts: Vec<String> = if req.self_hosts.is_empty() {
            url.as_ref()
                .and_then(|u| u.host_str().map(|x| x.to_lowercase()))
                .into_iter()
                .collect()
        } else {
            req.self_hosts.iter().map(|x| x.to_lowercase()).collect()
        };
        let (rscheme, rhost, rpath) = parse_ref(h.referer());
        let internal = !rhost.is_empty()
            && self_hosts
                .iter()
                .any(|s| rhost == *s || rhost.ends_with(&format!(".{s}")));

        // 1. UTM (GA4 waterfall)
        let (us, um, uc) = (
            qget_ci("utm_source"),
            qget_ci("utm_medium"),
            qget_ci("utm_campaign"),
        );
        if us.is_some() || um.is_some() {
            let (ch, _) =
                self.ga4_channel(us.unwrap_or(""), um.unwrap_or(""), uc.unwrap_or(""), &rhost);
            let full = us.is_some() && um.is_some();
            let src = us
                .map(String::from)
                .or_else(|| client.app.clone())
                .unwrap_or_else(|| {
                    if rhost.is_empty() {
                        "(not set)".into()
                    } else {
                        rhost.clone()
                    }
                });
            push(
                &mut ev,
                SignalKind::Utm,
                format!("utm:{}/{}", us.unwrap_or(""), um.unwrap_or("")),
                if full {
                    weights::UTM_FULL
                } else {
                    weights::UTM_PARTIAL
                },
                Some(ch),
                Some(src.clone()),
                Some(self.ga4_src.clone()),
                Some("2026-10-24".into()),
            );
            meta.insert(
                (ch, norm_source(&src)),
                (
                    um.unwrap_or("(not set)").to_string(),
                    uc.map(String::from),
                    if ch.paid() == Paid::Paid {
                        Interaction::AdClick
                    } else {
                        Interaction::Click
                    },
                ),
            );
        }
        // 2. click IDs. Matched case-insensitively: vendors are inconsistent about casing
        // (Snapchat documents `ScCid` but emits `sccid`), and the loader rejects rules that
        // would collide under that comparison.
        for c in &self.click_ids {
            // The signal reports the parameter as it appeared in the URL, not as the rule spells
            // it, so the evidence describes the request.
            let Some(seen) = qkey_ci(&q, &c.param) else {
                continue;
            };
            if internal {
                continue;
            }
            let mut ch = c.channel;
            let mut source = c.source.clone();
            if c.source == "google" && rhost.contains("youtube") && !c.weak {
                ch = Channel::PaidVideo;
                source = "youtube".into();
            }
            let (kind, w) = if c.weak {
                (SignalKind::WeakClickId, weights::WEAK_CLICK_ID)
            } else {
                (SignalKind::ClickId, weights::CLICK_ID)
            };
            // fbclid etc. say "came from Meta" but not paid vs organic: source-only for weak IDs on social
            let chan = if c.weak && ch == Channel::OrganicSocial {
                None
            } else {
                Some(ch)
            };
            push(
                &mut ev,
                kind,
                format!("click_id:{seen}"),
                w,
                chan,
                Some(source.clone()),
                c.src.clone(),
                c.verified.clone(),
            );
            meta.entry((ch, norm_source(&source))).or_insert((
                c.medium.clone(),
                uc.map(String::from),
                if ch.paid() == Paid::Paid {
                    Interaction::AdClick
                } else {
                    Interaction::Click
                },
            ));
        }
        // 3. in-app UA token / X-Requested-With / android-app referrer
        let ua = h.ua();
        if !internal {
            let hua = Haystack::new(ua);
            if let Some(t) = self.ua_tokens.matching(&hua).next() {
                push(
                    &mut ev,
                    SignalKind::UaToken,
                    format!("ua_token:{}", t.r.source),
                    weights::UA_TOKEN,
                    None,
                    Some(t.r.source.clone()),
                    t.r.src.clone(),
                    t.r.verified.clone(),
                );
                let ch = type_channel(&t.r.kind, false);
                meta.entry((ch, norm_source(&t.r.source))).or_insert((
                    medium_for(&t.r.kind),
                    None,
                    Interaction::Click,
                ));
            }
            if let Some(x) = h.get("x-requested-with").filter(|x| !is_browser_pkg(x)) {
                let src = self
                    .xrw_apps
                    .get(x)
                    .cloned()
                    .unwrap_or_else(|| x.to_string());
                let kind = if is_mail_pkg(x) {
                    "email"
                } else if search_src(&src) {
                    "search"
                } else {
                    "social"
                };
                push(
                    &mut ev,
                    SignalKind::XRequestedWith,
                    format!("x-requested-with:{x}"),
                    weights::X_REQUESTED_WITH,
                    None,
                    Some(src.clone()),
                    Some(self.xrw_src.clone()),
                    Some("2026-10-24".into()),
                );
                meta.entry((type_channel(kind, false), norm_source(&src)))
                    .or_insert((medium_for(kind), None, Interaction::Click));
            }
        }
        // 4. share markers (organic social)
        if !internal {
            for m in &self.share_markers {
                let p = m.param.as_str();
                let Some(v) = qget(p) else { continue };
                if m.value_rx.as_ref().is_some_and(|r| !r.is_match(v)) {
                    continue;
                }
                if m.ref_host_rx.as_ref().is_some_and(|r| !r.is_match(&rhost)) {
                    continue;
                }
                // `withParam` makes this marker conditional on a sibling parameter; `t` alone is
                // far too common to mean anything, so it only counts alongside `s`.
                if m.with_param.as_deref().is_some_and(|w| qget(w).is_none()) {
                    continue;
                }
                if p == "t" && m.with_param.is_none() {
                    continue;
                }
                if p == "s"
                    && qget("t").is_none()
                    && !re!(r"(^|\.)(t\.co|x\.com|twitter\.com)$").is_match(&rhost)
                {
                    continue;
                }
                let src = match m.source.as_str() {
                    "youtube_or_spotify" => {
                        if rhost.contains("spotify") {
                            "spotify".to_string()
                        } else {
                            "youtube".to_string()
                        }
                    }
                    x => x.to_string(),
                };
                push(
                    &mut ev,
                    SignalKind::ShareMarker,
                    format!("share_marker:{p}"),
                    weights::SHARE_MARKER,
                    Some(Channel::OrganicSocial),
                    Some(src.clone()),
                    m.src.clone(),
                    m.verified.clone(),
                );
                meta.entry((Channel::OrganicSocial, norm_source(&src)))
                    .or_insert(("social".into(), None, Interaction::Share));
                break;
            }
        }
        // 5. referrer
        if rscheme == "android-app" && !rhost.is_empty() {
            let src = self
                .xrw_apps
                .get(&rhost)
                .cloned()
                .unwrap_or_else(|| rhost.clone());
            let kind = if search_src(&src) {
                "search"
            } else if is_mail_pkg(&rhost) {
                "email"
            } else {
                "social"
            };
            let ch = type_channel(kind, false);
            push(&mut ev, SignalKind::AndroidAppReferrer, format!("android-app:{rhost}"), weights::ANDROID_APP_REF, Some(ch), Some(src.clone()), Some("https://developer.chrome.com/docs/android/custom-tabs/guide-interactions (EXTRA_REFERRER android-app://)".into()), None);
            meta.entry((ch, norm_source(&src))).or_insert((
                medium_for(kind),
                None,
                Interaction::Click,
            ));
        } else if internal {
            push(
                &mut ev,
                SignalKind::Internal,
                format!("referer:{rhost}"),
                weights::INTERNAL,
                Some(Channel::Internal),
                Some("(self)".into()),
                None,
                None,
            );
            meta.insert(
                (Channel::Internal, "(self)".into()),
                ("internal".into(), None, Interaction::Internal),
            );
        } else if !rhost.is_empty() {
            self.referrer_evidence(&rhost, &rpath, &mut ev, &mut meta);
        } else if ev.iter().all(|e| e.weight <= 0.0) {
            // Zero-weight evidence (the prefetch marker) carries no attribution, so it must not
            // suppress this branch — a prefetched cross-site navigation is still dark social.
            // 6. no referrer, no params
            let site = h.get("sec-fetch-site").unwrap_or("").to_ascii_lowercase();
            let mdn = Some("https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Sec-Fetch-Site".to_string());
            if site == "cross-site" {
                push(
                    &mut ev,
                    SignalKind::SecFetch,
                    "sec-fetch-site:cross-site".into(),
                    weights::DARK_SOCIAL_SECFETCH,
                    Some(Channel::DarkSocial),
                    Some("(referrer stripped)".into()),
                    mdn.clone(),
                    None,
                );
                meta.insert(
                    (Channel::DarkSocial, "(referrer stripped)".into()),
                    ("(none)".into(), None, Interaction::Click),
                );
            } else if site == "same-origin" || site == "same-site" {
                push(
                    &mut ev,
                    SignalKind::SecFetch,
                    format!("sec-fetch-site:{site}"),
                    0.6,
                    Some(Channel::Internal),
                    Some("(self)".into()),
                    mdn.clone(),
                    None,
                );
                meta.insert(
                    (Channel::Internal, "(self)".into()),
                    ("internal".into(), None, Interaction::Internal),
                );
            }
            if client.surface == Surface::WebviewIab {
                push(
                    &mut ev,
                    SignalKind::NoReferrer,
                    "webview_without_referrer".into(),
                    weights::DARK_SOCIAL_WEBVIEW,
                    Some(Channel::DarkSocial),
                    Some(
                        client
                            .app
                            .clone()
                            .unwrap_or_else(|| "(in-app, unidentified)".into()),
                    ),
                    None,
                    None,
                );
                meta.insert(
                    (
                        Channel::DarkSocial,
                        norm_source(
                            &client
                                .app
                                .clone()
                                .unwrap_or_else(|| "(in-app, unidentified)".into()),
                        ),
                    ),
                    ("(none)".into(), None, Interaction::Click),
                );
            }
            let w = if site == "none" || site.is_empty() {
                weights::DIRECT
            } else {
                0.3
            };
            push(
                &mut ev,
                SignalKind::NoReferrer,
                "no_referer".into(),
                w,
                Some(Channel::Direct),
                Some("(direct)".into()),
                Some("https://support.google.com/analytics/answer/9756891".into()),
                Some("2026-10-24".into()),
            );
            meta.insert(
                (Channel::Direct, "(direct)".into()),
                ("(none)".into(), None, Interaction::Navigation),
            );
        }
        if ev.iter().all(|e| e.kind == SignalKind::Prefetch) {
            push(
                &mut ev,
                SignalKind::NoReferrer,
                "no_signal".into(),
                0.3,
                Some(Channel::Direct),
                Some("(direct)".into()),
                None,
                None,
            );
            meta.insert(
                (Channel::Direct, "(direct)".into()),
                ("(none)".into(), None, Interaction::Navigation),
            );
        }
        let new_touch = !internal && !is_prefetch;
        self.finish(ev, meta, client, None, is_prefetch, new_touch)
    }

    fn referrer_evidence(
        &self,
        rhost: &str,
        rpath: &str,
        ev: &mut Vec<Evidence>,
        meta: &mut HashMap<(Channel, String), (String, Option<String>, Interaction)>,
    ) {
        let bare = rhost.trim_start_matches("www.");
        let mut add = |kind,
                       sig: String,
                       w: f64,
                       ch: Channel,
                       src: String,
                       medium: &str,
                       rs: Option<String>,
                       ver: Option<String>| {
            // organic referrers are source-only evidence: a google.com referrer also supports paid_search/google
            let chan = if kind == SignalKind::Referrer
                && ch.paid() == Paid::Organic
                && matches!(
                    ch,
                    Channel::OrganicSearch | Channel::OrganicSocial | Channel::OrganicVideo
                ) {
                None
            } else {
                Some(ch)
            };
            ev.push(Evidence {
                kind,
                signal: sig,
                weight: w,
                channel: chan,
                source: Some(norm_source(&src)),
                rule_src: rs,
                verified: ver,
            });
            meta.entry((ch, norm_source(&src))).or_insert((
                medium.to_string(),
                None,
                if ch.paid() == Paid::Paid {
                    Interaction::AdClick
                } else {
                    Interaction::Click
                },
            ));
        };
        if let Some((_, _, src)) = self
            .ai_paths
            .iter()
            .find(|(h, p, _)| (rhost == h || bare == h) && p.is_match(rpath))
        {
            add(
                SignalKind::AiPathReferrer,
                format!("referer:{rhost}{rpath}"),
                weights::AI_PATH,
                Channel::AiAssistant,
                src.clone(),
                "ai-assistant",
                None,
                None,
            );
            return;
        }
        if self.mastodon.contains(bare) {
            add(
                SignalKind::Referrer,
                format!("referer:{rhost}:mastodon"),
                0.7,
                Channel::OrganicSocial,
                "mastodon".into(),
                "social",
                Some("https://github.com/matomo-org/searchengine-and-social-list (CC0)".into()),
                None,
            );
            return;
        }
        if let Some(r) = self.referrers.first(rhost) {
            let r = &r.r;
            if r.source == "google" && re!(r"^(mail|docs|drive)\.").is_match(rhost) {
                let mail = rhost.starts_with("mail");
                add(
                    SignalKind::Referrer,
                    format!("referer:{rhost}"),
                    weights::REFERRER,
                    if mail {
                        Channel::Email
                    } else {
                        Channel::Referral
                    },
                    rhost.to_string(),
                    if mail { "email" } else { "referral" },
                    r.src.clone(),
                    r.verified.clone(),
                );
                return;
            }
            let paid = r.paid
                || r.kind == "display"
                || (r.source == "google"
                    && (rpath.starts_with("/aclk") || rpath.starts_with("/pagead")));
            let ch = type_channel(&r.kind, paid);
            let medium = if paid {
                "cpc".to_string()
            } else {
                medium_for(&r.kind)
            };
            add(
                SignalKind::Referrer,
                format!("referer:{rhost}"),
                weights::REFERRER,
                ch,
                r.source.clone(),
                &medium,
                r.src.clone(),
                r.verified.clone(),
            );
            if let Some(p) = self.search_keyword_param(rhost, bare, rpath) {
                // Confirms the referrer is a results page for this engine rather than its home
                // page. It says nothing about paid vs organic — an ad is clicked from the same
                // results page — so it is source-only evidence, like an organic referrer.
                ev.push(Evidence {
                    kind: SignalKind::SearchKeyword,
                    signal: format!("search_keyword:{p}"),
                    weight: weights::SEARCH_KEYWORD,
                    channel: None,
                    source: Some(norm_source(&r.source)),
                    rule_src: Some(
                        "https://github.com/matomo-org/searchengine-and-social-list (CC0)".into(),
                    ),
                    verified: None,
                });
            }
            return;
        }
        if let Some(e) = self.external.get(rhost).or_else(|| self.external.get(bare)) {
            let m = e.medium.clone().unwrap_or_else(|| "referral".into());
            let kind = match m.as_str() {
                "search" | "social" | "email" => m.as_str(),
                "paid" => "display",
                _ => "referral",
            };
            add(
                SignalKind::ExternalList,
                format!("external_list:{}", e.source),
                weights::EXTERNAL_LIST,
                type_channel(kind, false),
                e.source.to_lowercase(),
                &m,
                Some("external file (user-supplied)".into()),
                None,
            );
            return;
        }
        if let Some((name, kind, _)) = self.oss_lookup(rhost) {
            add(
                SignalKind::OssList,
                format!("oss_list:{name}"),
                weights::OSS_LIST,
                type_channel(&kind, false),
                name.to_lowercase(),
                &medium_for(&kind),
                Some("https://github.com/matomo-org/searchengine-and-social-list (CC0)".into()),
                None,
            );
            return;
        }
        add(
            SignalKind::Referrer,
            format!("referer:{rhost}"),
            weights::UNKNOWN_REFERRER,
            Channel::Referral,
            bare.to_string(),
            "referral",
            None,
            None,
        );
    }

    /// Name of the engine's documented search-keyword parameter, if the referrer carries it with a
    /// non-empty value. Only the parameter name is returned: the query itself is the visitor's, and
    /// this crate does not put it in its output.
    fn search_keyword_param(&self, rhost: &str, bare: &str, rpath: &str) -> Option<&str> {
        let query = rpath.split_once('?')?.1;
        let params = self
            .keyword_params
            .get(bare)
            .or_else(|| self.keyword_params.get(rhost))?;
        let present: HashSet<&str> = query
            .split('&')
            .filter_map(|kv| {
                let (k, v) = kv.split_once('=')?;
                (!v.is_empty()).then_some(k)
            })
            .collect();
        params
            .iter()
            .find(|p| present.contains(p.as_str()))
            .map(|p| p.as_str())
    }

    fn oss_lookup(&self, host: &str) -> Option<(String, String, bool)> {
        let mut h = host.trim_start_matches("www.").to_string();
        let tld = re!(r"\.[a-z]{2,3}(\.[a-z]{2})?$");
        while h.contains('.') {
            if let Some(v) = self.oss.get(&h) {
                return Some(v.clone());
            }
            let base = tld.replace_first(&h, "");
            if let Some(v) = self.oss.get(&base).filter(|v| v.2) {
                return Some(v.clone());
            }
            h = h[h.find('.').unwrap() + 1..].to_string();
        }
        None
    }

    fn finish(
        &self,
        ev: Vec<Evidence>,
        meta: HashMap<(Channel, String), (String, Option<String>, Interaction)>,
        client: Client,
        bot: Option<Bot>,
        is_prefetch: bool,
        new_touch: bool,
    ) -> Attribution {
        // hypotheses = every (channel, source) named by evidence or meta
        let mut hyps: Vec<(Channel, String)> = meta.keys().cloned().collect();
        for e in &ev {
            if let (Some(c), Some(s)) = (e.channel, &e.source) {
                if !hyps.contains(&(c, s.clone())) {
                    hyps.push((c, s.clone()));
                }
            }
        }
        let mut cands: Vec<Candidate> =
            hyps.into_iter()
                .map(|(ch, src)| {
                    let mut prod = 1.0;
                    let mut idx = vec![];
                    for (i, e) in ev.iter().enumerate() {
                        if e.weight <= 0.0 || e.channel.is_some_and(|c| c != ch) {
                            continue;
                        }
                        let f = match &e.source {
                            None => 1.0,
                            Some(s) if *s == src => 1.0,
                            Some(s) if family(s).is_some() && family(s) == family(&src) => {
                                weights::FAMILY_FACTOR
                            }
                            _ => continue,
                        };
                        // source-only evidence must be compatible with the channel
                        if e.channel.is_none()
                            && matches!(ch, Channel::Direct | Channel::Internal | Channel::Bot)
                        {
                            continue;
                        }
                        prod *= 1.0 - e.weight * f;
                        idx.push(i);
                    }
                    // conflict discount: explicit tagging (UTM / click ID) for the same source but another
                    // channel lowers this hypothesis by (1 - CONFLICT_FACTOR * w)
                    for e in &ev {
                        if matches!(e.kind, SignalKind::Utm | SignalKind::ClickId)
                            && e.channel.is_some_and(|c| c != ch)
                            && e.source.as_ref().is_some_and(|s| {
                                *s == src || (family(s).is_some() && family(s) == family(&src))
                            })
                        {
                            prod = 1.0 - (1.0 - prod) * (1.0 - weights::CONFLICT_FACTOR * e.weight);
                        }
                    }
                    let (medium, campaign, inter) = meta
                        .get(&(ch, src.clone()))
                        .cloned()
                        .unwrap_or(("(not set)".into(), None, Interaction::Click));
                    Candidate {
                        channel: ch,
                        source: src,
                        medium,
                        campaign,
                        paid: ch.paid(),
                        interaction: if is_prefetch {
                            Interaction::Prefetch
                        } else {
                            inter
                        },
                        confidence: round(1.0 - prod),
                        share: 0.0,
                        evidence: idx,
                    }
                })
                .filter(|c| c.confidence > 0.0)
                .collect();
        if cands.is_empty() {
            cands.push(Candidate {
                channel: Channel::Unknown,
                source: "(unknown)".into(),
                medium: "(not set)".into(),
                campaign: None,
                paid: Paid::Unknown,
                interaction: Interaction::Unknown,
                confidence: 0.1,
                share: 0.0,
                evidence: vec![],
            });
        }
        cands.sort_by(|a, b| {
            // total_cmp, not partial_cmp().unwrap(): ordering must never panic on a result.
            b.confidence
                .total_cmp(&a.confidence)
                .then(a.paid.cmp(&b.paid))
                .then(a.channel.cmp(&b.channel))
                .then_with(|| a.source.cmp(&b.source))
        });
        let total: f64 = cands.iter().map(|c| c.confidence).sum();
        for c in &mut cands {
            c.share = round(c.confidence / total);
        }
        Attribution {
            candidates: cands,
            client,
            bot,
            is_prefetch,
            new_touch,
            evidence: ev,
        }
    }

    /// GA4 default channel group waterfall (support.google.com/analytics/answer/9756891), incl. the
    /// "AI Assistant" channel; plus Plausible's paid-by-utm_source rule. Returns (channel, paid).
    pub fn ga4_channel(
        &self,
        source: &str,
        medium: &str,
        campaign: &str,
        ref_host: &str,
    ) -> (Channel, bool) {
        let (s, m, c) = (
            source.to_lowercase(),
            medium.to_lowercase(),
            campaign.to_lowercase(),
        );
        let pm = self.ga4.paid_medium.is_match(&m);
        let ps = self.ga4.paid_source.is_match(&s);
        // Strip the ad suffix only from a source that is actually an ad source. Stripping
        // unconditionally turned `threads` into `thre` and lost the social category with it.
        let stripped = if s == "adwords" {
            "google".to_string()
        } else if ps {
            re!(r"[-_. ]?ads?$").replace_first(&s, "")
        } else {
            s.clone()
        };
        let cat = self.source_category(&stripped, ref_host);
        let paid = pm || ps;
        use Channel::*;
        let r = |c: Channel| (c, c.paid() == Paid::Paid);
        if (s == "(direct)" || s == "direct") && (m.is_empty() || m == "(none)" || m == "(not set)")
        {
            return r(Direct);
        }
        if c.contains("cross-network") {
            return r(CrossNetwork);
        }
        if m == "ai-assistant" || c == "(ai-assistant)" {
            return r(AiAssistant);
        }
        let shop = cat == "shopping" || self.ga4.shop_camp.is_match(&c);
        if shop && paid {
            return r(PaidShopping);
        }
        match (cat.as_str(), paid) {
            ("search", true) => return r(PaidSearch),
            ("social", true) => return r(PaidSocial),
            ("video", true) => return r(PaidVideo),
            ("ai", true) => return r(PaidAi),
            _ => {}
        }
        if self.ga4.display.is_match(&m) {
            return r(Display);
        }
        if paid {
            return r(PaidOther);
        }
        if shop {
            return r(OrganicShopping);
        }
        if cat == "social" || self.ga4.social_medium.is_match(&m) {
            return r(OrganicSocial);
        }
        if cat == "video" || m.contains("video") {
            return r(OrganicVideo);
        }
        if cat == "ai" {
            return r(AiAssistant);
        }
        if cat == "search" || m == "organic" {
            return r(OrganicSearch);
        }
        let em = re!(r"e[-_ ]?mail|newsletter");
        if em.is_match(&s) || em.is_match(&m) || cat == "email" {
            return r(Email);
        }
        if m == "affiliate" {
            return r(Affiliate);
        }
        if m == "audio" {
            return r(Audio);
        }
        if s == "sms" || m == "sms" {
            return r(Sms);
        }
        if m.ends_with("push")
            || m.contains("mobile")
            || m.contains("notification")
            || s == "firebase"
        {
            return r(MobilePush);
        }
        if m == "referral" || m == "app" || m == "link" || !s.is_empty() {
            return r(Referral);
        }
        r(Unknown)
    }

    fn source_category(&self, s: &str, ref_host: &str) -> String {
        let s = s.trim_start_matches("www.");
        if ai_src(s) {
            return "ai".into();
        }
        if self.ga4.shop_host.is_match(s)
            || re!(r"^(google[ _]shopping|igshopping|shopping)$").is_match(s)
        {
            return "shopping".into();
        }
        if self.ga4.video_host.is_match(s) || re!(r"^(youtube|yt|vimeo|twitch|kick)$").is_match(s) {
            return "video".into();
        }
        let r = self.referrers.first(s).or_else(|| {
            if ref_host.is_empty() {
                None
            } else {
                self.referrers.first(ref_host)
            }
        });
        if search_src(s) || r.is_some_and(|r| r.r.kind == "search") {
            return "search".into();
        }
        if social_src(s)
            || r.is_some_and(|r| r.r.kind == "social")
            || re!(r"^(fb|ig|x|twitter|tw|li|bluesky|bsky|mastodon|threads|meta)([-_. ].*)?$")
                .is_match(s)
        {
            return "social".into();
        }
        if re!(r"^(gmail|outlook|yahoo_mail|e-?mail|newsletter)$").is_match(s)
            || r.is_some_and(|r| r.r.kind == "email")
        {
            return "email".into();
        }
        self.oss_lookup(s).map(|o| o.1).unwrap_or_default()
    }
}

// ---------------------------------------------------------------- helpers

struct Headers<'a> {
    m: HashMap<String, &'a str>,
    req: &'a Request,
}
impl<'a> Headers<'a> {
    fn new(req: &'a Request) -> Self {
        Self {
            m: req
                .headers
                .iter()
                .map(|(k, v)| (k.to_ascii_lowercase(), v.as_str()))
                .collect(),
            req,
        }
    }
    fn get(&self, k: &str) -> Option<&'a str> {
        self.m.get(k).copied().filter(|v| !v.is_empty())
    }
    fn ua(&self) -> &'a str {
        self.req
            .user_agent
            .as_deref()
            .or_else(|| self.get("user-agent"))
            .unwrap_or("")
    }
    fn referer(&self) -> &'a str {
        self.req
            .referer
            .as_deref()
            .or_else(|| self.get("referer"))
            .or_else(|| self.get("referrer"))
            .filter(|r| *r != "-")
            .unwrap_or("")
    }
}

fn parse_url(req: &Request, h: &Headers) -> Option<Url> {
    Url::parse(&req.url).or_else(|| {
        let host = h
            .get("host")
            .map(String::from)
            .or_else(|| req.self_hosts.first().cloned())
            .unwrap_or_else(|| "localhost".into());
        Url::parse(&format!("https://{host}{}", req.url))
    })
}

fn parse_ref(r: &str) -> (String, String, String) {
    match Url::parse(r) {
        Some(u) => {
            let path = format!(
                "{}{}",
                u.path(),
                u.query().map(|q| format!("?{q}")).unwrap_or_default()
            );
            (
                u.scheme().to_string(),
                u.host_str().unwrap_or("").to_lowercase(),
                path,
            )
        }
        None => (String::new(), String::new(), String::new()),
    }
}

fn mk_bot(class: BotClass, token: &str, op: &str, src: Option<String>) -> Bot {
    Bot {
        class,
        token: token.to_string(),
        operator: op.to_lowercase(),
        exclude: class != BotClass::AiUserFetch,
        rule_src: src,
    }
}

fn is_browser_pkg(x: &str) -> bool {
    re!(r"(?i)^(com\.android\.chrome|com\.chrome\.(beta|dev|canary)|org\.mozilla\.\w+|com\.sec\.android\.app\.sbrowser|com\.opera\.\w+|com\.brave\.browser|com\.microsoft\.emmx|com\.UCMobile\.intl|com\.mi\.globalbrowser|com\.duckduckgo\.mobile\.android|com\.vivaldi\.browser|XMLHttpRequest)$").is_match(x)
}
fn is_mail_pkg(x: &str) -> bool {
    re!(r"(?i)^(com\.google\.android\.gm|com\.microsoft\.office\.outlook|com\.yahoo\.mobile\.client\.android\.mail|ch\.protonmail\.android|com\.samsung\.android\.email\.provider)$").is_match(x)
}
fn search_src(s: &str) -> bool {
    matches!(
        s,
        "google"
            | "bing"
            | "yahoo"
            | "yandex"
            | "baidu"
            | "duckduckgo"
            | "naver"
            | "ecosia"
            | "brave"
            | "daum"
            | "seznam"
            | "yahoo_japan"
    )
}
fn social_src(s: &str) -> bool {
    matches!(
        s,
        "facebook"
            | "instagram"
            | "messenger"
            | "threads"
            | "x"
            | "linkedin"
            | "tiktok"
            | "snapchat"
            | "pinterest"
            | "reddit"
            | "youtube"
            | "quora"
            | "vk"
            | "weibo"
            | "douyin"
            | "xiaohongshu"
            | "twitch"
            | "kick"
            | "bluesky"
            | "mastodon"
    )
}
fn ai_src(s: &str) -> bool {
    re!(r"^(chatgpt(\.com)?|openai|perplexity|gemini|copilot|claude|grok|deepseek|meta[_ .]?ai|poe|mistral|duck[_.]?ai|other_ai)$").is_match(s)
}
fn type_channel(t: &str, paid: bool) -> Channel {
    match (t, paid) {
        ("search", false) => Channel::OrganicSearch,
        ("search", true) => Channel::PaidSearch,
        ("social", false) => Channel::OrganicSocial,
        ("social", true) => Channel::PaidSocial,
        ("video", false) => Channel::OrganicVideo,
        ("video", true) => Channel::PaidVideo,
        ("email", _) => Channel::Email,
        ("ai", false) => Channel::AiAssistant,
        ("ai", true) => Channel::PaidAi,
        ("display", _) => Channel::Display,
        ("shopping", false) => Channel::OrganicShopping,
        ("shopping", true) => Channel::PaidShopping,
        _ => Channel::Referral,
    }
}
fn medium_for(t: &str) -> String {
    match t {
        "search" => "organic",
        "social" => "social",
        "email" => "email",
        "ai" => "ai-assistant",
        "video" => "video",
        _ => "referral",
    }
    .into()
}
fn norm_source(s: &str) -> String {
    let s = s.to_lowercase();
    match s.as_str() {
        "fb" | "facebook.com" | "m.facebook.com" | "l.facebook.com" => "facebook".into(),
        "ig" | "instagram.com" | "l.instagram.com" => "instagram".into(),
        "twitter" | "t.co" | "x.com" | "tw" => "x".into(),
        "chatgpt.com" => "chatgpt".into(),
        _ => s,
    }
}
fn family(s: &str) -> Option<&'static str> {
    match s {
        "facebook" | "instagram" | "messenger" | "threads" | "meta" | "whatsapp" | "meta_ai" => {
            Some("meta")
        }
        "google" | "youtube" | "gemini" | "gmail" => Some("google"),
        "chatgpt" | "openai" => Some("openai"),
        "bing" | "copilot" | "microsoft" | "linkedin" | "teams" | "outlook" => Some("microsoft"),
        "tiktok" | "douyin" | "bytedance" => Some("bytedance"),
        "x" | "grok" => Some("x"),
        _ => None,
    }
}
/// Quantise a score to three decimals in `0.0..=1.0`. NaN becomes 0.0 rather than propagating:
/// a NaN would serialise as JSON `null` and silently break every consumer downstream.
fn round(x: f64) -> f64 {
    if x.is_nan() {
        return 0.0;
    }
    (x.clamp(0.0, 1.0) * 1000.0).round() / 1000.0
}

// ---------------------------------------------------------------- log parsing

/// Parse one Apache/Nginx *combined* log line into a [`Request`] (host taken from `default_host`).
///
/// `"-"` in the referer or user-agent field is the servers' "not present" placeholder and becomes
/// `None`, so a line logged without a user agent is treated the same as a request that arrives
/// without one (and is therefore classed as automation, not as a human visit).
pub fn parse_combined_log(line: &str, default_host: &str) -> Option<Request> {
    let c = re!(
        r#"^\S+ \S+ \S+ \[[^\]]+\] "(?:[A-Z]+) (\S+)[^"]*" \d{3} \S+ "((?:[^"\\]|\\.)*)" "((?:[^"\\]|\\.)*)""#
    )
    .captures(line)?;
    let path = c[1].to_string();
    let url = if path.starts_with("http") {
        path
    } else {
        format!("https://{default_host}{path}")
    };
    let unlog = |s: &str| Some(s.replace("\\\"", "\"")).filter(|v| v != "-" && !v.is_empty());
    Some(Request {
        url,
        referer: unlog(&c[2]),
        user_agent: unlog(&c[3]),
        self_hosts: vec![default_host.to_string()],
        ..Default::default()
    })
}

// ---------------------------------------------------------------- unit tests
//
// Integration tests in `tests/` exercise the public surface. These cover the private helpers and
// the loader's validation, which are not reachable from outside the crate.

#[cfg(test)]
mod tests {
    use super::*;

    fn eng() -> &'static Attributor {
        static E: std::sync::OnceLock<Attributor> = std::sync::OnceLock::new();
        E.get_or_init(Attributor::new)
    }

    // ------------------------------------------------------------ enums

    #[test]
    fn enum_names_round_trip_through_serde() {
        macro_rules! round_trip {
            ($($t:ty: [$($v:expr),*]),* $(,)?) => {$($(
                let name = <$t>::as_str(&$v);
                assert!(!name.is_empty(), "{:?} serialises to an empty string", $v);
                assert_eq!(<$t>::parse(&name), Some($v), "{name} did not parse back");
                assert!(
                    name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                    "{name} is not snake_case"
                );
            )*)*};
        }
        round_trip!(
            Channel: [Channel::Direct, Channel::OrganicSearch, Channel::PaidAi, Channel::DarkSocial, Channel::Unknown],
            Paid: [Paid::Paid, Paid::Organic, Paid::Unknown],
            Interaction: [Interaction::Click, Interaction::AdClick, Interaction::AiUserFetch, Interaction::PreviewUnfurl],
            Surface: [Surface::StandaloneBrowser, Surface::WebviewIab, Surface::AmpCache, Surface::Twa],
            Os: [Os::Ios, Os::MetaHorizonOs, Os::NintendoSwitch, Os::Unknown],
            FormFactor: [FormFactor::Phone, FormFactor::XrHeadset, FormFactor::EReader],
            BotClass: [BotClass::AiCrawler, BotClass::AiUserFetch, BotClass::SearchCrawler],
            SignalKind: [SignalKind::Utm, SignalKind::SearchKeyword, SignalKind::NoReferrer],
        );
    }

    #[test]
    fn unknown_enum_name_does_not_parse() {
        assert_eq!(Channel::parse("not_a_channel"), None);
        assert_eq!(
            Channel::parse("OrganicSearch"),
            None,
            "parse takes snake_case"
        );
        assert_eq!(Os::parse(""), None);
    }

    #[test]
    fn paid_classification_matches_the_channel_name() {
        for (ch, want) in [
            (Channel::PaidSearch, Paid::Paid),
            (Channel::PaidSocial, Paid::Paid),
            (Channel::PaidAi, Paid::Paid),
            (Channel::PaidShopping, Paid::Paid),
            (Channel::PaidVideo, Paid::Paid),
            (Channel::PaidOther, Paid::Paid),
            (Channel::Display, Paid::Paid),
            (Channel::CrossNetwork, Paid::Paid),
            (Channel::OrganicSearch, Paid::Organic),
            (Channel::Direct, Paid::Organic),
            (Channel::Bot, Paid::Unknown),
            (Channel::Unknown, Paid::Unknown),
        ] {
            assert_eq!(ch.paid(), want, "{}", ch.as_str());
        }
        // every channel whose name starts with "paid_" must report Paid
        for ch in [
            Channel::PaidSearch,
            Channel::PaidSocial,
            Channel::PaidVideo,
            Channel::PaidShopping,
            Channel::PaidOther,
            Channel::PaidAi,
        ] {
            assert!(ch.as_str().starts_with("paid_") && ch.paid() == Paid::Paid);
        }
    }

    // ------------------------------------------------------------ pure helpers

    #[test]
    fn norm_source_folds_aliases_and_lowercases() {
        for (input, want) in [
            ("FaceBook.com", "facebook"),
            ("l.facebook.com", "facebook"),
            ("fb", "facebook"),
            ("IG", "instagram"),
            ("t.co", "x"),
            ("Twitter", "x"),
            ("x.com", "x"),
            ("chatgpt.com", "chatgpt"),
            ("SomeBrand", "somebrand"),
        ] {
            assert_eq!(norm_source(input), want, "{input}");
        }
    }

    #[test]
    fn norm_source_is_idempotent() {
        for s in ["fb", "t.co", "chatgpt.com", "x.com", "anything"] {
            assert_eq!(norm_source(&norm_source(s)), norm_source(s), "{s}");
        }
    }

    #[test]
    fn family_groups_are_symmetric_and_disjoint() {
        assert_eq!(family("instagram"), family("facebook"));
        assert_eq!(family("youtube"), family("google"));
        assert_eq!(family("grok"), family("x"));
        assert_ne!(family("google"), family("openai"));
        assert_eq!(family("some_unknown_source"), None);
        // a source is never in two families
        for s in ["facebook", "google", "chatgpt", "bing", "tiktok", "x"] {
            assert!(family(s).is_some(), "{s}");
        }
    }

    #[test]
    fn type_channel_and_medium_agree_on_paid() {
        for kind in ["search", "social", "video", "shopping"] {
            assert_eq!(type_channel(kind, false).paid(), Paid::Organic, "{kind}");
            assert_eq!(type_channel(kind, true).paid(), Paid::Paid, "{kind}");
        }
        assert_eq!(type_channel("email", true), Channel::Email);
        assert_eq!(type_channel("display", false), Channel::Display);
        assert_eq!(type_channel("anything else", false), Channel::Referral);
        assert_eq!(medium_for("search"), "organic");
        assert_eq!(medium_for("ai"), "ai-assistant");
        assert_eq!(medium_for("mystery"), "referral");
    }

    #[test]
    fn round_clamps_and_quantises() {
        assert_eq!(round(0.123_456), 0.123);
        assert_eq!(round(1.7), 1.0, "clamped to 1");
        assert_eq!(round(-0.4), 0.0, "clamped to 0");
        assert_eq!(round(f64::NAN), 0.0, "NaN must not escape into a score");
        assert_eq!(round(f64::INFINITY), 1.0);
        for x in [0.0, 0.0005, 0.5, 0.9999, 1.0] {
            assert!((0.0..=1.0).contains(&round(x)), "{x}");
        }
    }

    #[test]
    fn parse_ref_splits_scheme_host_and_path() {
        assert_eq!(
            parse_ref("https://WWW.Example.com/a/b?q=1"),
            ("https".into(), "www.example.com".into(), "/a/b?q=1".into()),
            "host is lowercased, query is kept on the path"
        );
        assert_eq!(
            parse_ref("android-app://com.reddit.frontpage"),
            (
                "android-app".into(),
                "com.reddit.frontpage".into(),
                "".into()
            )
        );
        for junk in ["", "-", "not a url", "/relative/path", "://", "http://"] {
            assert_eq!(
                parse_ref(junk),
                (String::new(), String::new(), String::new()),
                "{junk:?}"
            );
        }
    }

    #[test]
    fn parse_url_falls_back_to_the_host_header_then_self_hosts() {
        let abs = Request {
            url: "https://a.example/p".into(),
            ..Default::default()
        };
        assert_eq!(
            parse_url(&abs, &Headers::new(&abs)).unwrap().host_str(),
            Some("a.example")
        );

        let mut rel = Request {
            url: "/p?x=1".into(),
            ..Default::default()
        };
        rel.headers.insert("Host".into(), "b.example".into());
        assert_eq!(
            parse_url(&rel, &Headers::new(&rel)).unwrap().host_str(),
            Some("b.example")
        );

        let self_host = Request {
            url: "/p".into(),
            self_hosts: vec!["c.example".into()],
            ..Default::default()
        };
        assert_eq!(
            parse_url(&self_host, &Headers::new(&self_host))
                .unwrap()
                .host_str(),
            Some("c.example")
        );

        let bare = Request {
            url: "/p".into(),
            ..Default::default()
        };
        assert_eq!(
            parse_url(&bare, &Headers::new(&bare)).unwrap().host_str(),
            Some("localhost")
        );
    }

    #[test]
    fn package_name_helpers() {
        assert!(is_browser_pkg("com.android.chrome"));
        assert!(is_browser_pkg("org.mozilla.firefox"));
        assert!(!is_browser_pkg("com.twitter.android"));
        assert!(
            !is_browser_pkg("com.android.chrome.evil"),
            "anchored at both ends"
        );
        assert!(is_mail_pkg("com.google.android.gm"));
        assert!(!is_mail_pkg("com.google.android.gm.extra"));
    }

    #[test]
    fn source_predicates() {
        assert!(search_src("google") && search_src("duckduckgo"));
        assert!(!search_src("Google"), "callers normalise first");
        assert!(social_src("instagram") && social_src("bluesky"));
        assert!(ai_src("chatgpt") && ai_src("chatgpt.com") && ai_src("perplexity"));
        assert!(!ai_src("chatgpt.com.evil.example"), "anchored");
        assert!(!ai_src("notchatgpt"));
    }

    #[test]
    fn first_ci_duplicate_finds_case_only_collisions() {
        assert_eq!(first_ci_duplicate(["a", "b", "c"].into_iter()), None);
        assert_eq!(
            first_ci_duplicate(["ScCid", "x", "sccid"].into_iter()),
            Some("sccid".into())
        );
        assert_eq!(first_ci_duplicate([].into_iter()), None);
    }

    #[test]
    fn qkey_ci_returns_the_key_as_written() {
        let q = vec![("ScCid".to_string(), "v".to_string())];
        assert_eq!(qkey_ci(&q, "sccid"), Some("ScCid"));
        assert_eq!(qkey_ci(&q, "SCCID"), Some("ScCid"));
        assert_eq!(qkey_ci(&q, "other"), None);
    }

    // ------------------------------------------------------------ headers

    #[test]
    fn headers_are_case_insensitive_and_treat_empty_as_absent() {
        let mut r = Request {
            url: "https://a.example/".into(),
            ..Default::default()
        };
        r.headers.insert("User-Agent".into(), "UA".into());
        r.headers.insert("X-Empty".into(), String::new());
        let h = Headers::new(&r);
        assert_eq!(h.get("user-agent"), Some("UA"));
        assert_eq!(
            h.get("x-empty"),
            None,
            "an empty header value is not a value"
        );
        assert_eq!(h.ua(), "UA");
    }

    #[test]
    fn explicit_fields_win_over_headers_and_dashes_are_dropped() {
        let mut r = Request {
            url: "https://a.example/".into(),
            user_agent: Some("explicit".into()),
            referer: Some("-".into()),
            ..Default::default()
        };
        r.headers.insert("user-agent".into(), "from-header".into());
        r.headers
            .insert("referrer".into(), "https://b.example/".into());
        let h = Headers::new(&r);
        assert_eq!(h.ua(), "explicit");
        assert_eq!(h.referer(), "", "\"-\" is the log placeholder for absent");

        // the US spelling is accepted as a fallback
        let r2 = Request {
            url: "https://a.example/".into(),
            ..Default::default()
        };
        let mut r2 = r2;
        r2.headers
            .insert("referrer".into(), "https://b.example/".into());
        assert_eq!(Headers::new(&r2).referer(), "https://b.example/");
    }

    // ------------------------------------------------------------ loader validation

    #[test]
    fn embedded_rules_load() {
        Attributor::from_json(RULES, DEVICES, OSS).expect("embedded rules must be valid");
    }

    #[test]
    fn loader_rejects_click_ids_that_collide_case_insensitively() {
        let rules = RULES.replace(r#""param": "twclid""#, r#""param": "GCLID""#);
        let err = Attributor::from_json(&rules, DEVICES, OSS).unwrap_err();
        assert!(err.to_string().contains("duplicate parameter"), "{err}");
    }

    #[test]
    fn loader_rejects_bot_tokens_that_collide_case_insensitively() {
        let rules = RULES.replace(r#""token": "GPTBot""#, r#""token": "gptbot""#);
        // the replacement itself creates no duplicate; add one
        let rules = rules.replacen(r#""token": "CCBot""#, r#""token": "GPTBOT""#, 1);
        let err = Attributor::from_json(&rules, DEVICES, OSS).unwrap_err();
        assert!(err.to_string().contains("duplicate token"), "{err}");
    }

    #[test]
    fn loader_rejects_an_unknown_channel_rather_than_defaulting() {
        let rules = RULES.replace(r#""channel": "paid_search""#, r#""channel": "pad_search""#);
        let err = Attributor::from_json(&rules, DEVICES, OSS).unwrap_err();
        assert!(err.to_string().contains("unknown channel"), "{err}");
    }

    #[test]
    fn loader_rejects_an_unknown_bot_class() {
        let rules = RULES.replace(r#""class": "ai_crawler""#, r#""class": "ai_crawlers""#);
        let err = Attributor::from_json(&rules, DEVICES, OSS).unwrap_err();
        assert!(err.to_string().contains("unknown class"), "{err}");
    }

    #[test]
    fn loader_rejects_an_uncompilable_regex_rather_than_ignoring_it() {
        let devices = DEVICES.replace(r#""re": "OculusBrowser"#, r#""re": "([unclosed"#);
        let err = Attributor::from_json(RULES, &devices, OSS).unwrap_err();
        assert!(
            err.to_string().contains("regex") || err.to_string().contains("unclosed"),
            "{err}"
        );
    }

    #[test]
    fn loader_reports_malformed_json() {
        let err = Attributor::from_json("{", DEVICES, OSS).unwrap_err();
        assert!(err.to_string().starts_with("rule error:"), "{err}");
    }

    #[test]
    fn external_referrers_accept_both_shapes_and_reject_junk() {
        let flat = r#"{"a.example": {"source": "A", "medium": "social"}}"#;
        let wrapped = r#"{"referrers": {"a.example": {"source": "A", "medium": "social"}}}"#;
        for j in [flat, wrapped] {
            Attributor::new().with_external_referrers(j).expect(j);
        }
        // extra keys are ignored, a missing `source` is not
        Attributor::new()
            .with_external_referrers(r#"{"a.example":{"source":"A","medium":null,"from":["x"]}}"#)
            .unwrap();
        assert!(Attributor::new()
            .with_external_referrers(r#"{"a.example": {"medium": "social"}}"#)
            .is_err());
        assert!(Attributor::new()
            .with_external_file("/no/such/file")
            .is_err());
    }

    // ------------------------------------------------------------ search keyword lookup

    #[test]
    fn search_keyword_param_needs_a_non_empty_value() {
        let e = eng();
        assert_eq!(
            e.search_keyword_param("www.google.com", "google.com", "/search?q=shoes"),
            Some("q")
        );
        assert_eq!(
            e.search_keyword_param("www.google.com", "google.com", "/search?q="),
            None
        );
        assert_eq!(
            e.search_keyword_param("www.google.com", "google.com", "/search"),
            None
        );
        assert_eq!(
            e.search_keyword_param("www.google.com", "google.com", "/"),
            None
        );
        assert_eq!(
            e.search_keyword_param("nope.example", "nope.example", "/?q=x"),
            None
        );
    }

    #[test]
    fn oss_lookup_walks_up_the_domain() {
        let e = eng();
        assert!(e.oss_lookup("facebook.com").is_some());
        assert!(e.oss_lookup("www.facebook.com").is_some());
        assert!(e.oss_lookup("some.deep.sub.facebook.com").is_some());
        assert!(e
            .oss_lookup("definitely-not-a-known-host.invalid")
            .is_none());
        // must terminate on pathological input
        assert!(e.oss_lookup("").is_none());
        assert!(e.oss_lookup(".").is_none());
        assert!(e.oss_lookup(&".".repeat(200)).is_none());
    }
}
