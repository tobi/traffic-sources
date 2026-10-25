// Builds the exact-host fallback table from Matomo searchengine-and-social-list (CC0 1.0) and the
// AI bot split from ai.robots.txt (MIT). Writes ./rules-oss.json and ./rules-aibots.json for the
// TypeScript reference, and ../../rules/oss-referrers.json for the Rust crate.
//
// Expects shallow clones of the upstream lists under docs/oss-survey/src/ (gitignored). Snowplow
// referers.yml is GPLv3 and is NOT vendored; read-only use of docs/oss-survey/.
//
// Usage: node reference/ts/build-oss-rules.mjs   (runnable from anywhere)
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repo = join(here, "..", "..");
const SURVEY = join(repo, "docs", "oss-survey", "src");
const D = join(SURVEY, "matomo-org__searchengine-and-social-list") + "/";
const TS_OSS = join(here, "rules-oss.json");
const TS_AIBOTS = join(here, "rules-aibots.json");
const CRATE_OSS = join(repo, "rules", "oss-referrers.json");
const hosts = {};
function add(type, name, line) {
  const h = line.trim().replace(/^-\s*/, "").replace(/^['"]|['"]$/g, "").toLowerCase();
  if (!h || h.includes("{}") || h.includes("/") && !h.startsWith("www")) { if (!h.includes("{}")) return; }
  hosts[h.replace(/^www\./, "").split("/")[0]] ??= { name, type };
}
for (const [file, type] of [["Socials.yml","social"],["AIAssistants.yml","ai"]]) {
  let name = "";
  for (const l of readFileSync(D + file, "utf8").split("\n")) {
    if (/^\S.*:\s*$/.test(l)) name = l.replace(/:\s*$/, "").replace(/^['"]|['"]$/g, "");
    else if (/^\s+-\s+\S/.test(l) && name) add(type, name, l);
  }
}
{ let name = "", inUrls = false;
  for (const l of readFileSync(D + "SearchEngines.yml", "utf8").split("\n")) {
    if (/^\S.*:\s*$/.test(l)) { name = l.replace(/:\s*$/, "").replace(/^['"]|['"]$/g, ""); inUrls = false; }
    else if (/^\s+urls:/.test(l)) inUrls = true;
    else if (/^\s+\w+:/.test(l)) inUrls = false;
    else if (inUrls && /^\s+-\s+\S/.test(l)) add("search", name, l);
  }
}
for (const k of Object.keys(hosts)) if (k.includes("{}")) { const v = hosts[k]; delete hosts[k]; hosts[k.replace(".{}", "")] = { ...v, prefix: true }; }
writeFileSync(TS_OSS, JSON.stringify({ source: "matomo-org/searchengine-and-social-list", license: "CC0-1.0", hosts }, null, 0));
const c = {}; for (const v of Object.values(hosts)) c[v.type] = (c[v.type] ?? 0) + 1; console.log(Object.keys(hosts).length, c);

// --- AI bots from ai.robots.txt (MIT): split user-triggered fetchers vs crawlers ---
const robots = JSON.parse(readFileSync(join(SURVEY, "ai-robots-txt__ai.robots.txt", "robots.json"), "utf8"));
const userFetchers = {}, crawlers = [];
for (const [name, v] of Object.entries(robots)) {
  const f = String(v.function ?? ""), fr = String(v.frequency ?? "");
  if (/^AI Assistants$/i.test(f) || /when prompted by a user|user[- ]initiated/i.test(fr) || /-User$/.test(name)) userFetchers[name] = String(v.operator ?? "").replace(/\[([^\]]+)\].*/, "$1");
  else crawlers.push(name);
}
writeFileSync(TS_AIBOTS, JSON.stringify({ source: "ai-robots-txt/ai.robots.txt", license: "MIT", userFetchers, crawlers }, null, 1));
console.log("aibots", Object.keys(userFetchers).length, "user fetchers,", crawlers.length, "crawlers");

// --- search keyword params from Matomo SearchEngines.yml (CC0) ---
{ const kw = {}; let name = "", sec = "", urls = [];
  const flush = (params) => { for (const u of urls) kw[u.replace(/^www\./, "")] = params; };
  let params = [];
  for (const l of readFileSync(D + "SearchEngines.yml", "utf8").split("\n")) {
    if (/^\S.*:\s*$/.test(l)) { if (urls.length && params.length) flush(params); urls = []; params = []; sec = ""; }
    else if (/^\s+-\s*$/.test(l)) { if (urls.length && params.length) flush(params); urls = []; params = []; sec = ""; }
    else if (/^\s+(urls|params):/.test(l)) sec = l.trim().replace(":", "");
    else if (/^\s+\w+:/.test(l)) sec = "";
    else if (/^\s+-\s+\S/.test(l)) { const v = l.trim().replace(/^-\s*/, "").replace(/^['"]|['"]$/g, ""); if (sec === "urls" && !v.includes("{}")) urls.push(v.toLowerCase()); if (sec === "params" && !v.startsWith("/")) params.push(v); }
  }
  if (urls.length && params.length) flush(params);
  const j = JSON.parse(readFileSync(TS_OSS, "utf8")); j.keywordParams = kw;
  writeFileSync(TS_OSS, JSON.stringify(j));
  // The Rust crate embeds the same table with include_str!.
  writeFileSync(CRATE_OSS, JSON.stringify(j));
  console.log("keyword param hosts", Object.keys(kw).length);
}
