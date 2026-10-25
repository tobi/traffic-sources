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

use regex::{Regex, RegexBuilder, RegexSet, RegexSetBuilder};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use url::Url;

/// Compile a literal regex once.
macro_rules! re {
    ($p:expr) => {{
        static R: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
        #[allow(clippy::regex_creation_in_loops)]
        R.get_or_init(|| Regex::new($p).unwrap())
    }};
}

const RULES: &str = include_str!("../rules/rules.json");
const DEVICES: &str = include_str!("../rules/devices.json");
const OSS: &str = include_str!("../rules/oss-referrers.json");

// ---------------------------------------------------------------- taxonomy

macro_rules! str_enum {
    ($(#[$m:meta])* $name:ident { $($v:ident),* $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($v),* }
        impl $name {
            /// snake_case name as serialised.
            pub fn as_str(&self) -> String { serde_json::to_value(self).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default() }
            /// Parse a snake_case name.
            pub fn parse(s: &str) -> Option<Self> { serde_json::from_value(Value::String(s.to_string())).ok() }
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
    Referrer, AiPathReferrer, ExternalList, OssList, SecFetch, NoReferrer, BotUa, BotHeader, Prefetch, Internal });

// ---------------------------------------------------------------- public I/O types

/// One request. Header names are matched case-insensitively.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Request {
    /// Absolute URL, or path (then `host` header / `self_hosts[0]` is used).
    pub url: String,
    #[serde(default)]
    pub referer: Option<String>,
    #[serde(default)]
    pub user_agent: Option<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// Your own hostnames (subdomains included). Defaults to the URL host.
    #[serde(default)]
    pub self_hosts: Vec<String>,
    /// Your own Android package names (TWA detection).
    #[serde(default)]
    pub self_android_packages: Vec<String>,
}

/// One signal with its documented source.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Evidence {
    pub kind: SignalKind,
    /// e.g. `click_id:gclid`, `ua_token:instagram`.
    pub signal: String,
    /// Likelihood weight in 0..1 used in the noisy-OR.
    pub weight: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<Channel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Documentation URL of the rule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_src: Option<String>,
    /// Date the rule was verified against `rule_src` (None = unverified).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified: Option<String>,
}

/// A ranked attribution hypothesis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub channel: Channel,
    pub source: String,
    pub medium: String,
    #[serde(skip_serializing_if = "Option::is_none")]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Client {
    pub os: Os,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_version: Option<String>,
    /// UA-reduction froze the version (macOS 10_15_7, Android "K" model).
    pub os_version_frozen: bool,
    pub form_factor: FormFactor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub browser: Option<String>,
    pub surface: Surface,
    /// Host app for in-app browsers / referring app.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    pub client_hints_used: bool,
    pub confidence: f64,
    /// Documentation of the device rule that matched.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_src: Option<String>,
}

/// Bot verdict.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bot {
    pub class: BotClass,
    pub token: String,
    pub operator: String,
    /// True if the request should be excluded from human analytics.
    pub exclude: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_src: Option<String>,
}

/// Full result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attribution {
    /// All candidates, highest confidence first. Never empty.
    pub candidates: Vec<Candidate>,
    pub client: Client,
    #[serde(skip_serializing_if = "Option::is_none")]
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

#[derive(Debug, Clone, Deserialize)]
struct ClickIdRule {
    param: String,
    source: String,
    medium: String,
    channel: String,
    #[serde(default)]
    weak: bool,
    src: Option<String>,
    verified: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
struct UaTokenRaw {
    re: String,
    source: String,
    #[serde(rename = "type")]
    kind: String,
    src: Option<String>,
    verified: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
struct RefRaw {
    re: String,
    source: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    paid: bool,
    src: Option<String>,
    verified: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
struct BotRaw {
    class: String,
    token: String,
    operator: String,
    src: Option<String>,
    verified: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
struct BotHeaderRaw {
    header: String,
    contains: String,
    class: String,
    operator: String,
    src: Option<String>,
    verified: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
struct DeviceRaw {
    re: String,
    os: String,
    form_factor: Option<String>,
    device: Option<String>,
    src: Option<String>,
    verified: Option<String>,
}

struct Compiled<T> {
    rx: Regex,
    r: T,
}

fn rx(p: &str, ci: bool) -> Result<Regex, RuleError> {
    RegexBuilder::new(p)
        .case_insensitive(ci)
        .build()
        .map_err(|e| RuleError(format!("{p}: {e}")))
}

/// External referrer entry (e.g. GPL-licensed lists you load yourself; never shipped).
#[derive(Debug, Clone, Deserialize)]
pub struct ExternalReferrer {
    pub source: String,
    #[serde(default)]
    pub medium: Option<String>,
}

/// The attribution engine. Construct once, reuse (it is `Send + Sync`).
pub struct Attributor {
    click_ids: Vec<ClickIdRule>,
    ua_tokens: Vec<Compiled<UaTokenRaw>>,
    xrw_apps: HashMap<String, String>,
    xrw_src: String,
    referrers: Vec<Compiled<RefRaw>>,
    ai_paths: Vec<(String, Regex, String)>,
    share_markers: Vec<Value>,
    mastodon: HashSet<String>,
    bots: Vec<BotRaw>,
    bot_set: RegexSet,
    bot_headers: Vec<BotHeaderRaw>,
    devices: Vec<Compiled<DeviceRaw>>,
    browsers: Vec<(Regex, String)>,
    ch_platform: HashMap<String, String>,
    ch_form: HashMap<String, String>,
    ga4: Ga4,
    oss: HashMap<String, (String, String, bool)>,
    keyword_params: HashMap<String, Vec<String>>,
    external: HashMap<String, ExternalReferrer>,
    ga4_src: String,
}

struct Ga4 {
    paid_medium: Regex,
    shop_camp: Regex,
    display: Regex,
    social_medium: Regex,
    paid_source: Regex,
    shop_host: Regex,
    video_host: Regex,
}

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
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
    pub fn from_json(rules: &str, devices: &str, oss: &str) -> Result<Self, RuleError> {
        let e = |x: serde_json::Error| RuleError(x.to_string());
        let r: Value = serde_json::from_str(rules).map_err(e)?;
        let d: Value = serde_json::from_str(devices).map_err(e)?;
        let o: Value = serde_json::from_str(oss).map_err(e)?;
        let click_ids: Vec<ClickIdRule> =
            serde_json::from_value(r["clickIds"].clone()).map_err(e)?;
        let ua_tokens = serde_json::from_value::<Vec<UaTokenRaw>>(r["uaTokens"].clone())
            .map_err(e)?
            .into_iter()
            .map(|t| {
                Ok(Compiled {
                    rx: rx(&t.re, true)?,
                    r: t,
                })
            })
            .collect::<Result<Vec<_>, RuleError>>()?;
        let xrw_apps: HashMap<String, String> =
            serde_json::from_value(r["xRequestedWith"]["apps"].clone()).map_err(e)?;
        let referrers = serde_json::from_value::<Vec<RefRaw>>(r["referrers"].clone())
            .map_err(e)?
            .into_iter()
            .map(|t| {
                Ok(Compiled {
                    rx: rx(&t.re, true)?,
                    r: t,
                })
            })
            .collect::<Result<Vec<_>, RuleError>>()?;
        let ai_paths = r["aiPathReferrers"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|a| Ok((s(a, "host"), rx(&s(a, "path"), false)?, s(a, "source"))))
            .collect::<Result<Vec<_>, RuleError>>()?;
        let bots: Vec<BotRaw> = serde_json::from_value(r["botRules"].clone()).map_err(e)?;
        let pats: Vec<String> = bots
            .iter()
            .map(|b| {
                let t = regex::escape(&b.token);
                let tail = if b.token.ends_with(|c: char| c.is_alphanumeric()) {
                    "\\b"
                } else {
                    ""
                };
                format!("\\b{t}{tail}")
            })
            .collect();
        let bot_set = RegexSetBuilder::new(&pats)
            .case_insensitive(true)
            .build()
            .map_err(|x| RuleError(x.to_string()))?;
        let g = &r["ga4"];
        let ga4 = Ga4 {
            paid_medium: rx(&s(g, "paidMedium"), false)?,
            shop_camp: rx(&s(g, "shoppingCampaign"), false)?,
            display: rx(&s(g, "display"), false)?,
            social_medium: rx(&s(g, "socialMedium"), false)?,
            paid_source: rx(&s(g, "paidSource"), true)?,
            shop_host: rx(&s(g, "shoppingHosts"), true)?,
            video_host: rx(&s(g, "videoHosts"), true)?,
        };
        let devices = serde_json::from_value::<Vec<DeviceRaw>>(d["devices"].clone())
            .map_err(e)?
            .into_iter()
            .map(|t| {
                Ok(Compiled {
                    rx: rx(&t.re, false)?,
                    r: t,
                })
            })
            .collect::<Result<Vec<_>, RuleError>>()?;
        let browsers = d["browsers"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|b| Ok((rx(&s(b, "re"), false)?, s(b, "name"))))
            .collect::<Result<Vec<_>, RuleError>>()?;
        let mut oss = HashMap::new();
        if let Some(h) = o["hosts"].as_object() {
            for (k, v) in h {
                oss.insert(
                    k.clone(),
                    (
                        s(v, "name"),
                        s(v, "type"),
                        v["prefix"].as_bool().unwrap_or(false),
                    ),
                );
            }
        }
        Ok(Self {
            click_ids,
            ua_tokens,
            xrw_apps,
            xrw_src: s(&r["xRequestedWith"], "_src"),
            referrers,
            ai_paths,
            share_markers: r["shareMarkers"].as_array().cloned().unwrap_or_default(),
            mastodon: serde_json::from_value(r["mastodonInstances"].clone()).unwrap_or_default(),
            bots,
            bot_set,
            bot_headers: serde_json::from_value(r["botHeaders"].clone()).map_err(e)?,
            devices,
            browsers,
            ch_platform: serde_json::from_value(d["clientHints"]["platform_map"].clone())
                .map_err(e)?,
            ch_form: serde_json::from_value(d["clientHints"]["form_factor_map"].clone())
                .map_err(e)?,
            ga4,
            oss,
            keyword_params: serde_json::from_value(o["keywordParams"].clone()).unwrap_or_default(),
            external: HashMap::new(),
            ga4_src: s(g, "_src"),
        })
    }

    /// Add an external referrer list `{host: {source, medium}}` or `{"referrers": {...}}`.
    /// Use this for GPL/AGPL/LGPL-derived lists, which this crate does not ship.
    pub fn with_external_referrers(mut self, json: &str) -> Result<Self, RuleError> {
        let v: Value = serde_json::from_str(json).map_err(|x| RuleError(x.to_string()))?;
        let m = v.get("referrers").cloned().unwrap_or(v);
        let map: HashMap<String, ExternalReferrer> =
            serde_json::from_value(m).map_err(|x| RuleError(x.to_string()))?;
        self.external.extend(map);
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
        if let Some(d) = self.devices.iter().find(|d| d.rx.is_match(ua)) {
            c.os = Os::parse(&d.r.os).unwrap_or(Os::Unknown);
            c.form_factor =
                d.r.form_factor
                    .as_deref()
                    .and_then(FormFactor::parse)
                    .unwrap_or(FormFactor::Unknown);
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
        c.browser = self
            .browsers
            .iter()
            .find(|(r, _)| r.is_match(ua))
            .map(|(_, n)| n.clone());
        // surface
        let iab = self.ua_tokens.iter().find(|t| t.rx.is_match(ua));
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
                && b.class != "prefetch"
            {
                let _ = &b.verified;
                return Some(mk_bot(&b.class, &b.header, &b.operator, b.src.clone()));
            }
        }
        let ua = h.ua();
        if ua.is_empty() {
            return Some(mk_bot("automation", "(empty user-agent)", "unknown", None));
        }
        let hits: Vec<usize> = self.bot_set.matches(ua).into_iter().collect();
        // prefer the longest token (Applebot-Extended over Applebot), and documented classes
        let best = hits
            .into_iter()
            .max_by_key(|&i| (self.bots[i].verified.is_some(), self.bots[i].token.len()))?;
        let b = &self.bots[best];
        Some(mk_bot(&b.class, &b.token, &b.operator, b.src.clone()))
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
                (b.class.as_str(), None, inter),
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
        let q: Vec<(String, String)> = url
            .as_ref()
            .map(|u| {
                u.query_pairs()
                    .map(|(a, b)| (a.into_owned(), b.into_owned()))
                    .collect()
            })
            .unwrap_or_default();
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
        // 2. click IDs
        for c in &self.click_ids {
            let present = if c.param == "ScCid" {
                qget_ci("sccid").is_some()
            } else {
                qget(&c.param).is_some()
            };
            if !present || internal {
                continue;
            }
            let mut ch = Channel::parse(&c.channel).unwrap_or(Channel::PaidOther);
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
                format!("click_id:{}", c.param),
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
            if let Some(t) = self.ua_tokens.iter().find(|t| t.rx.is_match(ua)) {
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
                let p = s(m, "param");
                let Some(v) = qget(&p) else { continue };
                if let Some(vr) = m.get("valueRe").and_then(|x| x.as_str()) {
                    if !Regex::new(vr).map(|r| r.is_match(v)).unwrap_or(false) {
                        continue;
                    }
                }
                if let Some(rh) = m.get("refHost").and_then(|x| x.as_str()) {
                    if !Regex::new(rh).map(|r| r.is_match(&rhost)).unwrap_or(false) {
                        continue;
                    }
                }
                if p == "t"
                    || (p == "s"
                        && qget("t").is_none()
                        && !re!(r"(^|\.)(t\.co|x\.com|twitter\.com)$").is_match(&rhost))
                {
                    continue;
                }
                let src = match s(m, "source").as_str() {
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
                    m.get("src").and_then(|x| x.as_str()).map(String::from),
                    None,
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
        } else if ev.is_empty() {
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
        if let Some(r) = self.referrers.iter().find(|r| r.rx.is_match(rhost)) {
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
            if let Some(kws) = self.keyword_params.get(bare) {
                let _ = kws; // keyword presence is informational only (search engines strip it)
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

    fn oss_lookup(&self, host: &str) -> Option<(String, String, bool)> {
        let mut h = host.trim_start_matches("www.").to_string();
        let tld = re!(r"\.[a-z]{2,3}(\.[a-z]{2})?$");
        while h.contains('.') {
            if let Some(v) = self.oss.get(&h) {
                return Some(v.clone());
            }
            let base = tld.replace(&h, "").to_string();
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
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap()
                .then(a.paid.cmp(&b.paid))
                .then(a.channel.cmp(&b.channel))
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
        let stripped = if s == "adwords" {
            "google".to_string()
        } else {
            re!(r"[-_. ]?ads?$").replace(&s, "").to_string()
        };
        let cat = self.source_category(&stripped, ref_host);
        let pm = self.ga4.paid_medium.is_match(&m);
        let ps = self.ga4.paid_source.is_match(&s);
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
        let r = self
            .referrers
            .iter()
            .find(|x| x.rx.is_match(s))
            .or_else(|| {
                if ref_host.is_empty() {
                    None
                } else {
                    self.referrers.iter().find(|x| x.rx.is_match(ref_host))
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
    Url::parse(&req.url).ok().or_else(|| {
        let host = h
            .get("host")
            .map(String::from)
            .or_else(|| req.self_hosts.first().cloned())
            .unwrap_or_else(|| "localhost".into());
        Url::parse(&format!("https://{host}{}", req.url)).ok()
    })
}

fn parse_ref(r: &str) -> (String, String, String) {
    match Url::parse(r) {
        Ok(u) => {
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
        Err(_) => (String::new(), String::new(), String::new()),
    }
}

fn mk_bot(class: &str, token: &str, op: &str, src: Option<String>) -> Bot {
    let class = BotClass::parse(class).unwrap_or(BotClass::Automation);
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
fn round(x: f64) -> f64 {
    ((x.clamp(0.0, 1.0)) * 1000.0).round() / 1000.0
}

// ---------------------------------------------------------------- log parsing

/// Parse one Apache/Nginx *combined* log line into a [`Request`] (host taken from `default_host`).
pub fn parse_combined_log(line: &str, default_host: &str) -> Option<Request> {
    let re = Regex::new(r#"^\S+ \S+ \S+ \[[^\]]+\] "(?:[A-Z]+) (\S+)[^"]*" \d{3} \S+ "((?:[^"\\]|\\.)*)" "((?:[^"\\]|\\.)*)""#).unwrap();
    let c = re.captures(line)?;
    let path = c[1].to_string();
    let url = if path.starts_with("http") {
        path
    } else {
        format!("https://{default_host}{path}")
    };
    let referer = Some(c[2].to_string()).filter(|r| r != "-" && !r.is_empty());
    Some(Request {
        url,
        referer,
        user_agent: Some(c[3].replace("\\\"", "\"")),
        self_hosts: vec![default_host.to_string()],
        ..Default::default()
    })
}
