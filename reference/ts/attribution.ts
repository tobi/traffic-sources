// Traffic attribution from access-log fields + request headers.
// Precedence: UTM > click ID > IAB UA token / X-Requested-With > Referer > Sec-Fetch heuristics > direct.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

export type Channel = "paid_search"|"organic_search"|"paid_social"|"organic_social"|"display"|"video"|"email"|"affiliate"|"ai_assistant"|"referral"|"internal"|"direct"|"organic_shopping"|"paid_shopping"|"paid_video"|"organic_video"|"paid_other"|"paid_ai"|"cross_network"|"audio"|"sms"|"mobile_push"|"bot"|"unknown";
export type Surface = "in_app"|"mobile_web"|"desktop_web"|"native_app"|"bot";
export interface Req { url: string; headers: Record<string,string>; selfHosts?: string[]; }
export interface Attribution { channel: Channel; source: string; medium: string; campaign?: string; surface: Surface; paid: boolean; confidence: number; isBot: boolean; isPrefetch: boolean; newTouch: boolean; signals: string[]; }

const here = dirname(fileURLToPath(import.meta.url));
const R = JSON.parse(readFileSync(join(here, "rules.json"), "utf8"));
const uaRules = R.uaTokens.map((r: any) => ({ ...r, rx: new RegExp(r.re, "i") }));
const refRules = R.referrers.map((r: any) => ({ ...r, rx: new RegExp(r.re, "i") }));
const botRx = new RegExp(R.bots, "i");
// AI bots split (ai.robots.txt, MIT): user-triggered fetchers are a human proxy, crawlers are excluded.
let AIB: { userFetchers: Record<string,string>; crawlers: string[] } = { userFetchers: {}, crawlers: [] };
try { AIB = JSON.parse(readFileSync(join(here, "rules-aibots.json"), "utf8")); } catch { /* optional */ }
const esc = (x: string) => x.replace(/[.*+?^${}()|[\]\\/]/g, "\\$&");
const aiFetchRx = new RegExp("(" + [...Object.keys(R.aiUserFetchers), ...Object.keys(AIB.userFetchers)].map(esc).join("|") + ")", "i");
const aiCrawlRx = AIB.crawlers.length ? new RegExp("\\b(" + AIB.crawlers.map(esc).join("|") + ")\\b", "i") : /$^/;
const G = { paidMedium: new RegExp(R.ga4.paidMedium), shopCamp: new RegExp(R.ga4.shoppingCampaign), display: new RegExp(R.ga4.display), socialMedium: new RegExp(R.ga4.socialMedium), paidSource: new RegExp(R.ga4.paidSource, "i"), shopHost: new RegExp(R.ga4.shoppingHosts, "i"), videoHost: new RegExp(R.ga4.videoHosts, "i") };
const MASTO = new Set<string>(R.mastodonInstances ?? []);
// Optional external rules (e.g. GPL/AGPL-derived oss-survey/data/merged.json). Never shipped; load explicitly.
let EXT: Record<string, { source: string; medium: string | null }> = {};
export function loadExternalReferrers(path: string) { const j = JSON.parse(readFileSync(path, "utf8")); EXT = j.referrers ?? j; }
let KW: Record<string, string[]> = {};
try { KW = JSON.parse(readFileSync(join(here, "rules-oss.json"), "utf8")).keywordParams ?? {}; } catch { /* optional */ }
// Fallback host table from Matomo searchengine-and-social-list (CC0), built by build-oss-rules.mjs
let OSS: Record<string, { name: string; type: string; prefix?: boolean }> = {};
try { OSS = JSON.parse(readFileSync(join(here, "rules-oss.json"), "utf8")).hosts; } catch { /* optional */ }
function ossLookup(host: string) {
  let h = host.replace(/^www\./, "");
  while (h.includes(".")) { if (OSS[h]) return OSS[h]; const base = h.replace(/\.[a-z]{2,3}(\.[a-z]{2})?$/, ""); if (OSS[base]?.prefix) return OSS[base]; h = h.slice(h.indexOf(".") + 1); }
  return undefined;
}
const BROWSER_PKGS = /^(com\.android\.chrome|com\.chrome\.(beta|dev|canary)|org\.mozilla\.\w+|com\.sec\.android\.app\.sbrowser|com\.opera\.\w+|com\.brave\.browser|com\.microsoft\.emmx|com\.UCMobile\.intl|com\.mi\.globalbrowser|XMLHttpRequest)$/i;
const SOCIAL_SRC = new Set(["facebook","instagram","messenger","threads","x","linkedin","tiktok","snapchat","pinterest","reddit","youtube","quora","vk","weibo","douyin","xiaohongshu","twitch","kick"]);
const SEARCH_SRC = new Set(["google","bing","yahoo","yandex","baidu","duckduckgo","naver","ecosia","brave","daum","seznam"]);
const AI_SRC = new Set(["chatgpt","chatgpt.com","perplexity","gemini","copilot","claude","grok","deepseek","meta_ai","meta.ai","duck_ai","poe","mistral","other_ai"]);

const H = (h: Record<string,string>, k: string) => { const f = Object.keys(h).find(x => x.toLowerCase() === k); return f ? h[f] : ""; };

function mediumToChannel(medium: string, source: string): { channel: Channel; paid: boolean } {
  const m = medium.toLowerCase(), s = source.toLowerCase();
  const social = SOCIAL_SRC.has(s) || /social|^sm$|^fb$|^ig$|facebook|instagram|tiktok|linkedin|twitter|pinterest|reddit|snap/.test(s);
  if (/^(cpc|ppc|paidsearch|paid_search|sem)$/.test(m)) return { channel: social ? "paid_social" : "paid_search", paid: true };
  if (/^(paid|paid_?social|paidsocial|cpm|cpv|ad|ads|sponsored)/.test(m) || (/paid|cp[cmav]/.test(m) && social)) return { channel: social ? "paid_social" : "display", paid: true };
  if (/display|banner|programmatic|native|interstitial|cpm/.test(m)) return { channel: "display", paid: true };
  if (/video/.test(m)) return { channel: "video", paid: /paid|cp/.test(m) };
  if (/^e[-_ ]?mail|newsletter|^crm$|lifecycle/.test(m) || /^(e[-_ ]?mail|newsletter|klaviyo|mailchimp)$/.test(s)) return { channel: "email", paid: false };
  if (/affiliate|partner/.test(m)) return { channel: "affiliate", paid: true };
  if (/^organic$/.test(m)) return { channel: SEARCH_SRC.has(s) ? "organic_search" : "organic_social", paid: false };
  if (/social|^sm$|share|post|story|bio|profile/.test(m) || social) return { channel: "organic_social", paid: false };
  if (/^referral$/.test(m)) return { channel: "referral", paid: false };
  if (AI_SRC.has(s) || /chatgpt|openai|perplexity/.test(s)) return { channel: "ai_assistant", paid: false };
  if (SEARCH_SRC.has(s)) return { channel: "organic_search", paid: false };
  return { channel: "referral", paid: false };
}

function sourceCategory(src: string, refHost: string): string {
  const s = src.toLowerCase().replace(/^www\./, "");
  const r = refRules.find((x: any) => x.rx.test(s)) ?? (refHost ? refRules.find((x: any) => x.rx.test(refHost)) : undefined);
  if (AI_SRC.has(s) || /chatgpt|openai|perplexity|claude|gemini|copilot|deepseek|grok|meta[_ .]?ai|poe|mistral|duck\.?ai/.test(s)) return "ai";
  if (G.shopHost.test(s) || /^(google[ _]shopping|igshopping|shopping)$/.test(s)) return "shopping";
  if (G.videoHost.test(s) || /^(youtube|yt|vimeo|twitch|kick|tiktok_video)$/.test(s)) return "video";
  if (SEARCH_SRC.has(s) || r?.type === "search") return "search";
  if (SOCIAL_SRC.has(s) || r?.type === "social" || /^(fb|ig|x|twitter|tw|li|bluesky|bsky|mastodon|threads|meta)([-_. ].*)?$/.test(s)) return "social";
  if (/^(gmail|outlook|yahoo_mail|e-?mail|newsletter)$/.test(s) || r?.type === "email") return "email";
  const o = ossLookup(s); if (o) return o.type;
  return "";
}

// GA4 default channel group order (support.google.com/analytics/answer/9756891), plus Plausible's
// AI Assistants channel and paid-by-utm_source rule (generalised: utm_source ending in ad/ads, or adwords).
export function ga4Channel(source: string, medium: string, campaign: string, refHost = ""): { channel: Channel; paid: boolean } {
  const s = source.toLowerCase(), m = medium.toLowerCase(), c = campaign.toLowerCase();
  const cat = sourceCategory(s.replace(/[-_. ]?ads?$|^adwords$/, (x: string) => x === "adwords" ? "google" : ""), refHost);
  const pm = G.paidMedium.test(m), ps = G.paidSource.test(s);
  if ((s === "(direct)" || s === "direct") && (m === "" || m === "(none)" || m === "(not set)")) return { channel: "direct", paid: false };
  if (c.includes("cross-network")) return { channel: "cross_network", paid: true };
  if ((cat === "shopping" || G.shopCamp.test(c)) && (pm || ps)) return { channel: "paid_shopping", paid: true };
  if (cat === "search" && (pm || ps)) return { channel: "paid_search", paid: true };
  if (cat === "social" && (pm || ps)) return { channel: "paid_social", paid: true };
  if (cat === "video" && (pm || ps)) return { channel: "paid_video", paid: true };
  if (cat === "ai" && (pm || ps)) return { channel: "paid_ai", paid: true };
  if (G.display.test(m)) return { channel: "display", paid: true };
  if (pm || ps) return { channel: "paid_other", paid: true };
  if (cat === "shopping" || G.shopCamp.test(c)) return { channel: "organic_shopping", paid: false };
  if (cat === "social" || G.socialMedium.test(m)) return { channel: "organic_social", paid: false };
  if (cat === "video" || m.includes("video")) return { channel: "organic_video", paid: false };
  if (cat === "ai") return { channel: "ai_assistant", paid: false };
  if (cat === "search" || m === "organic") return { channel: "organic_search", paid: false };
  if (/e[-_ ]?mail|newsletter/.test(s) || /e[-_ ]?mail|newsletter/.test(m) || cat === "email") return { channel: "email", paid: false };
  if (m === "affiliate") return { channel: "affiliate", paid: true };
  if (m === "audio") return { channel: "audio", paid: false };
  if (s === "sms" || m === "sms") return { channel: "sms", paid: false };
  if (m.endsWith("push") || m.includes("mobile") || m.includes("notification") || s === "firebase") return { channel: "mobile_push", paid: false };
  if (m === "referral" || m === "app" || m === "link" || s) return { channel: "referral", paid: false };
  return { channel: "unknown", paid: false };
}

function typeToChannel(type: string, paid: boolean): Channel {
  switch (type) {
    case "search": return paid ? "paid_search" : "organic_search";
    case "social": return paid ? "paid_social" : "organic_social";
    case "video": return "video"; case "email": return "email"; case "ai": return "ai_assistant"; case "display": return "display";
    default: return "referral";
  }
}

export function attribute(req: Req): Attribution {
  const sig: string[] = [];
  const h = req.headers;
  const ua = H(h, "user-agent"), ref = H(h, "referer") || H(h, "referrer"), xrw = H(h, "x-requested-with");
  const u = new URL(req.url);
  const q = u.searchParams;
  const self = (req.selfHosts ?? [u.hostname]).map(s => s.toLowerCase());

  // 0. bots, AI fetchers, prefetch
  const purpose = (H(h, "sec-purpose") || H(h, "purpose") || H(h, "x-purpose") || H(h, "x-moz")).toLowerCase();
  const isPrefetch = /prefetch|prerender|preview/.test(purpose);
  if (isPrefetch) sig.push(`prefetch:${purpose}`);
  const fm = ua.match(aiFetchRx);
  if (fm) { const tok = fm[1]; const src = (R.aiUserFetchers as Record<string,string>)[tok] ?? AIB.userFetchers[tok] ?? tok; return { channel: "bot", source: String(src).toLowerCase(), medium: "ai_user_fetch", surface: "bot", paid: false, confidence: 0.95, isBot: true, isPrefetch, newTouch: false, signals: [...sig, `ai_fetcher:${tok}`, "user_triggered"] }; }
  const cm = ua.match(aiCrawlRx);
  if (cm) return { channel: "bot", source: cm[1].toLowerCase(), medium: "ai_crawler", surface: "bot", paid: false, confidence: 0.95, isBot: true, isPrefetch, newTouch: false, signals: [...sig, `ai_crawler:${cm[1]}`, "exclude"] };
  if (!ua || botRx.test(ua)) return { channel: "bot", source: (ua.match(/([A-Za-z-]*bot|facebookexternalhit|WhatsApp|HeadlessChrome|curl|python-requests)/i)?.[1] ?? "unknown").toLowerCase(), medium: "crawler", surface: "bot", paid: false, confidence: 0.9, isBot: true, isPrefetch, newTouch: false, signals: [...sig, "bot_ua"] };

  // surface
  const iab = uaRules.find((r: any) => r.rx.test(ua));
  const xrwApp = xrw && !BROWSER_PKGS.test(xrw) ? xrw : "";
  const xrwSrc = xrwApp ? (R.xRequestedWith[xrwApp] ?? xrwApp) : "";
  let refHost = "", refPath = "", refScheme = "";
  if (ref) { try { const r = new URL(ref); refScheme = r.protocol; refHost = r.hostname.toLowerCase(); refPath = r.pathname + r.search; } catch { sig.push("bad_referer"); } }
  const androidApp = refScheme === "android-app:" ? refHost : "";
  let surface: Surface = /Mobi|Android|iPhone|iPod/i.test(ua) ? "mobile_web" : "desktop_web";
  if (iab || xrwApp || /; wv\)/.test(ua)) surface = "in_app";
  else if (androidApp) surface = "native_app";
  if (iab) sig.push(`ua_token:${iab.source}`);
  if (xrwApp) sig.push(`x-requested-with:${xrwApp}`);
  if (androidApp) sig.push(`android-app:${androidApp}`);

  const base = { surface, isBot: false, isPrefetch, signals: sig };
  const internal = refHost && self.some(s => refHost === s || refHost.endsWith("." + s));

  // 1. UTM
  const us = q.get("utm_source"), um = q.get("utm_medium"), uc = q.get("utm_campaign") ?? undefined;
  const ids = (R.clickIds as any[]).filter(c => q.has(c.param));
  const strongId = ids.find(c => !c.weak);
  if (us || um) {
    sig.push(`utm:${us ?? ""}/${um ?? ""}`);
    let { channel, paid } = ga4Channel(us ?? "", um ?? "", uc ?? "", refHost);
    if (strongId) { sig.push(`click_id:${strongId.param}`); if (!paid && strongId.medium !== "email" && strongId.medium !== "affiliate") { paid = true; channel = strongId.channel; sig.push("conflict:utm_organic_vs_paid_click_id->paid"); } }
    if (!us && iab) sig.push("utm_source_missing_filled_from_ua");
    return { ...base, channel, source: (us ?? iab?.source ?? xrwSrc ?? refHost) || "unknown", medium: um ?? "(not set)", campaign: uc, paid, confidence: um && us ? 0.98 : 0.85, newTouch: true };
  }
  // 2. click IDs
  if (strongId) {
    sig.push(`click_id:${strongId.param}`);
    let source = strongId.source, channel = strongId.channel as Channel;
    // gclid on a YouTube / social referer → Google video/display
    if (strongId.source === "google" && /youtube/.test(refHost)) { channel = "video"; source = "youtube"; }
    return { ...base, channel, source, medium: strongId.medium, paid: !["email"].includes(strongId.medium), confidence: 0.95, newTouch: true };
  }
  // 3. IAB / XRW
  const appSrc = iab?.source ?? xrwSrc;
  if (appSrc && !(internal)) {
    const weak = ids.find(c => c.weak);
    if (weak) sig.push(`click_id:${weak.param}`);
    const type = iab?.type ?? (SEARCH_SRC.has(appSrc) ? "search" : appSrc === "gmail" || appSrc === "outlook" ? "email" : "social");
    // Google app (GSA) is a browser too: only trust search if referer is google or empty
    if (appSrc === "google" && refHost && !/google\./.test(refHost)) { /* fall through to referer */ }
    else return { ...base, channel: typeToChannel(type, false), source: appSrc, medium: type === "search" ? "organic" : type === "email" ? "email" : "social", paid: false, confidence: iab ? 0.9 : 0.85, newTouch: true };
  }
  // weak click IDs (fbclid, igshid, srsltid)
  const weak = ids.find(c => c.weak);
  if (weak && !internal) {
    sig.push(`click_id:${weak.param}`);
    return { ...base, channel: weak.channel, source: weak.source, medium: weak.medium, paid: false, confidence: 0.8, newTouch: true };
  }
  // share markers (organic social): si on YouTube/Spotify referers, X s=<n>[&t=], ref_src=twsrc
  if (!internal) for (const m of (R.shareMarkers ?? []) as any[]) {
    const v = q.get(m.param); if (v === null) continue;
    if (m.valueRe && !new RegExp(m.valueRe).test(v)) continue;
    if (m.withParam && !q.has(m.withParam)) continue;
    if (m.refHost && !new RegExp(m.refHost).test(refHost)) continue;
    if (m.param === "t" || (m.param === "s" && !q.has("t") && !/(^|\.)(t\.co|x\.com|twitter\.com)$/.test(refHost))) continue;
    const src = m.source === "youtube_or_spotify" ? (/spotify/.test(refHost) ? "spotify" : "youtube") : m.source;
    sig.push(`share_marker:${m.param}`);
    return { ...base, channel: "organic_social", source: src, medium: "social", paid: false, confidence: 0.75, newTouch: true };
  }
  // 4. referer
  if (androidApp) {
    const src = R.xRequestedWith[androidApp] ?? androidApp;
    const type = src === "google" ? "search" : SEARCH_SRC.has(src) ? "search" : /gm$|outlook/.test(androidApp) ? "email" : "social";
    return { ...base, channel: typeToChannel(type, false), source: src, medium: type === "search" ? "organic" : "referral", paid: false, confidence: 0.8, newTouch: true };
  }
  if (refHost) {
    if (internal) return { ...base, channel: "internal", source: refHost, medium: "internal", paid: false, confidence: 0.9, newTouch: false };
    const ap = (R.aiPathReferrers as any[]).find(a => (refHost === a.host || refHost === "www." + a.host) && new RegExp(a.path).test(refPath));
    if (ap) { sig.push(`referer:${refHost}${refPath}`); return { ...base, channel: "ai_assistant", source: ap.source, medium: "referral", paid: false, confidence: 0.8, newTouch: true }; }
    if (MASTO.has(refHost.replace(/^www\./, ""))) { sig.push(`referer:${refHost}`, "mastodon_instance"); return { ...base, channel: "organic_social", source: "mastodon", medium: "social", paid: false, confidence: 0.75, newTouch: true }; }
    const kwp = KW[refHost.replace(/^www\./, "")];
    if (kwp) { const rq = new URLSearchParams(refPath.split("?")[1] ?? ""); const kp = kwp.find((p: string) => rq.get(p)); if (kp) sig.push(`search_keyword_param:${kp}`); }
    const r = refRules.find((x: any) => x.rx.test(refHost));
    if (r) {
      sig.push(`referer:${refHost}`);
      let paid = false, channel = typeToChannel(r.type, false);
      if (r.source === "google" && /\/aclk|\/pagead/.test(refPath)) { paid = true; channel = "paid_search"; }
      if (r.type === "display") paid = true;
      if (r.paid) { paid = true; channel = typeToChannel(r.type, true); }
      if (r.source === "google" && /^(mail|docs|drive)\./.test(refHost)) { return { ...base, channel: refHost.startsWith("mail") ? "email" : "referral", source: refHost, medium: refHost.startsWith("mail") ? "email" : "referral", paid: false, confidence: 0.8, newTouch: true }; }
      return { ...base, channel, source: r.source, medium: paid ? "cpc" : r.type === "search" ? "organic" : r.type === "ai" ? "referral" : r.type, paid, confidence: 0.75, newTouch: true };
    }
    sig.push(`referer:${refHost}`);
    const e = EXT[refHost] ?? EXT[refHost.replace(/^www\./, "")];
    if (e) { sig.push(`external_list:${e.source}`); const t = e.medium === "search" ? "search" : e.medium === "social" ? "social" : e.medium === "email" ? "email" : e.medium === "paid" ? "display" : "referral"; return { ...base, channel: typeToChannel(t, false), source: e.source.toLowerCase(), medium: e.medium ?? "referral", paid: e.medium === "paid", confidence: 0.65, newTouch: true }; }
    const o = ossLookup(refHost);
    if (o) { sig.push(`oss_list:${o.name}`); return { ...base, channel: typeToChannel(o.type, false), source: o.name.toLowerCase(), medium: o.type === "search" ? "organic" : o.type === "ai" ? "referral" : o.type, paid: false, confidence: 0.7, newTouch: true }; }
    return { ...base, channel: "referral", source: refHost, medium: "referral", paid: false, confidence: 0.7, newTouch: true };
  }
  // 5. Sec-Fetch heuristics
  const site = H(h, "sec-fetch-site").toLowerCase(), user = H(h, "sec-fetch-user"), mode = H(h, "sec-fetch-mode");
  if (site) sig.push(`sec-fetch-site:${site}`);
  if (site === "cross-site") return { ...base, channel: "unknown", source: "(stripped referer)", medium: "dark_social", paid: false, confidence: 0.4, newTouch: true };
  if (site === "same-origin" || site === "same-site") return { ...base, channel: "internal", source: "(self)", medium: "internal", paid: false, confidence: 0.6, newTouch: false };
  if (surface === "in_app") return { ...base, channel: "unknown", source: "(in-app, unidentified)", medium: "dark_social", paid: false, confidence: 0.4, newTouch: true };
  return { ...base, channel: "direct", source: "(direct)", medium: "(none)", paid: false, confidence: site === "none" && user === "?1" ? 0.7 : 0.5, newTouch: true, signals: mode ? [...sig, `mode:${mode}`] : sig };
}

// Session/first-touch: last non-direct touch wins within a session (GA-style); first touch kept separately.
export function sessionize(touches: Attribution[]): { first?: Attribution; last?: Attribution } {
  const real = touches.filter(t => !t.isBot && !t.isPrefetch && t.newTouch);
  const nonDirect = real.filter(t => t.channel !== "direct");
  return { first: real[0], last: nonDirect.at(-1) ?? real.at(-1) };
}
