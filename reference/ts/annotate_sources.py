# One-shot generator that produced the shipped ../../rules/*.json from this prototype's rules
# (v2 -> v3, critic loop 2026-10-24). Kept for provenance; it is not idempotent and reruns on the
# already-migrated v3 data will fail. The shipped rules are now edited in place -- see CONTRIBUTING.md.
#
# Attach {src, verified} to every rule. verified = date checked against the cited doc in this critic loop;
# null = not re-checked against a primary doc (status "unverified").
import json, os, re
HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "..", "rules")
V="2026-10-24"
R=json.load(open(os.path.join(HERE, "rules.json")))
AIB=json.load(open(os.path.join(HERE, "rules-aibots.json")))
CID={
 "gclid":("https://support.google.com/google-ads/answer/3095550",V),"gbraid":("https://support.google.com/google-ads/answer/3095550",V),
 "wbraid":("https://support.google.com/google-ads/answer/3095550",V),"gclsrc":("https://support.google.com/google-ads/answer/3095550",None),
 "gad_source":("https://support.google.com/google-ads/answer/16193746",V),"gad_campaignid":("https://support.google.com/google-ads/answer/16193746",V),
 "dclid":("https://support.google.com/campaignmanager/answer/9182069",None),
 "msclkid":("https://learn.microsoft.com/en-us/advertising/msa-help/hlp_ba_proc_microsoftclickid",V),
 "ttclid":("https://business-api.tiktok.com/portal/docs/setup-guide-for-web/v1.3",V),
 "ScCid":("https://businesshelp.snapchat.com/s/article/troubleshooting-click-id",V),"sccid":("https://businesshelp.snapchat.com/s/article/troubleshooting-click-id",V),
 "twclid":("https://leadtrackr.io/learn/advertising-click-ids",V),
 "li_fat_id":("https://learn.microsoft.com/en-us/linkedin/marketing/conversions/enabling-first-party-cookies",V),
 "epik":("https://help.pinterest.com/en/business/article/pinterest-tag-parameters-and-cookies",V),
 "rdt_cid":("https://leadtrackr.io/learn/advertising-click-ids",V),
 "fbclid":("https://developers.facebook.com/docs/marketing-api/conversions-api/parameters/fbp-and-fbc/",V),
 "srsltid":("https://support.google.com/merchants/answer/15191080",V),
 "oppref":("https://help.openai.com/en/articles/20001409-conversion-measurement",V),
}
for c in R["clickIds"]:
    s,v=CID.get(c["param"],("https://github.com/matomo-org/searchengine-and-social-list (CC0) / oss-survey click_ids",None))
    c["src"]=s; c["verified"]=v
UA_SRC={
 "x":("https://github.com/ua-parser/uap-core/blob/master/regexes.yaml + https://docs.uaparser.dev/info/browser/name/twitter.html",V),
 "facebook":("https://explore.whatismybrowser.com/useragents/explore/software_name/facebook-app/1",V),
 "instagram":("https://dev.to/abdullah_afzal/how-to-detect-in-app-browsers-instagram-tiktok-facebook-1a31",V),
}
for t in R["uaTokens"]:
    s,v=UA_SRC.get(t["source"],("https://github.com/ua-parser/uap-core/blob/master/regexes.yaml ; https://github.com/shalanah/inapp-spy",None))
    t["src"]=s; t["verified"]=v
R["xRequestedWith"]={"_src":"https://developer.android.com/jetpack/androidx/releases/webkit (1.15.0: XRW removal cancelled, header sent on all requests again); https://chromestatus.com/feature/5160086884843520","_verified":V,"apps":R["xRequestedWith"]}
for r in R["referrers"]:
    r.setdefault("src","https://github.com/matomo-org/searchengine-and-social-list (CC0) + curated"); r.setdefault("verified",None)
    if r.get("type")=="ai": r["src"]="https://support.google.com/analytics/answer/9756891 (AI Assistant channel; list not published)"; r["verified"]=V
for a in R["aiPathReferrers"]: a["src"]="curated"; a["verified"]=None
for m in R["shareMarkers"]: m["src"]="Observed on first-party share links emitted by the platform's own Share dialog; the parameter name is a bare fact, independently verifiable. Not derived from ClearURLs/Neat-URL/AdGuard (copyleft)."; m["verified"]=None
R["ga4"]["_src"]="https://support.google.com/analytics/answer/9756891"; R["ga4"]["_verified"]=V
# bots: classify
BOTS={
 "ai_user_fetch":{"ChatGPT-User":"chatgpt","Claude-User":"claude","Perplexity-User":"perplexity","MistralAI-User":"mistral","Meta-ExternalFetcher":"meta_ai","DuckAssistBot":"duckduckgo","Google-Agent":"google","Gemini-Deep-Research":"gemini","Google-NotebookLM":"google","kagi-fetcher":"kagi","Kimi-User":"kimi","Manus-User":"manus"},
 "ai_search_crawler":{"OAI-SearchBot":"openai","Claude-SearchBot":"anthropic","PerplexityBot":"perplexity","Meta-WebIndexer":"meta","Applebot":"apple"},
 "ai_crawler":{"GPTBot":"openai","ClaudeBot":"anthropic","meta-externalagent":"meta","FacebookBot":"meta","Bytespider":"bytedance","CCBot":"commoncrawl","Amazonbot":"amazon","Google-CloudVertexBot":"google","Applebot-Extended":"apple"},
 "ad_verifier":{"OAI-AdsBot":"openai","Meta-ExternalAds":"meta","AdsBot-Google":"google"},
 "link_preview":{"facebookexternalhit":"facebook","Facebot":"facebook","Twitterbot":"x","LinkedInBot":"linkedin","Slackbot":"slack","Discordbot":"discord","TelegramBot":"telegram","WhatsApp/":"whatsapp","Pinterestbot":"pinterest","redditbot":"reddit","SkypeUriPreview":"skype","bingpreview":"bing","Iframely":"iframely","Embedly":"embedly"},
 "search_crawler":{"Googlebot":"google","bingbot":"bing","YandexBot":"yandex","Baiduspider":"baidu","DuckDuckBot":"duckduckgo","Slurp":"yahoo","SeznamBot":"seznam","Naverbot":"naver","Yeti/":"naver","Sogou":"sogou","PetalBot":"huawei"},
 "seo_tool":{"AhrefsBot":"ahrefs","SemrushBot":"semrush","MJ12bot":"majestic","DotBot":"moz","rogerbot":"moz","Screaming Frog":"screamingfrog","serpstatbot":"serpstat","DataForSeoBot":"dataforseo"},
 "monitoring":{"UptimeRobot":"uptimerobot","Pingdom":"pingdom","StatusCake":"statuscake","Site24x7":"site24x7","Better Uptime":"betterstack","Datadog":"datadog","NewRelicPinger":"newrelic","GoogleStackdriverMonitoring":"google","Checkly":"checkly"},
 "automation":{"HeadlessChrome":"headless","python-requests":"python","curl/":"curl","Go-http-client":"go","okhttp":"okhttp","Wget":"wget","axios/":"axios","node-fetch":"node","PhantomJS":"phantomjs","Lighthouse":"lighthouse"},
}
# ai.robots.txt's `operator` is free text ("unclear at this time.", "google that retrieves web
# content on behalf of gemini api users"). The engine emits the operator as the attribution
# `source`, so keep only a short slug and fall back to the bot's own name.
OPERATOR_OVERRIDE={"AI2Bot-DeepResearchEval":"ai2","amazon-QBusiness":"amazon","Amzn-User":"amazon",
 "bigsur.ai":"bigsur_ai","ElevenlabsBot":"elevenlabs","ExaUser":"exa","GeistHaus-PageFetcher":"geisthaus",
 "GoogleAgent-URLContext":"google","LinerBot":"liner","MistralAI-User/1.0":"mistral","NotebookLM":"google",
 "Poggio-Citations":"poggio","Shap-User":"shap","TongyiBot":"alibaba","UseAI":"useai","YiyanBot":"baidu",
 "Webzio-Extended":"webz_io"}
def slug(tok,op):
    if tok in OPERATOR_OVERRIDE: return OPERATOR_OVERRIDE[tok]
    op=str(op).strip().lower()
    return op if re.fullmatch(r"[a-z0-9][a-z0-9_.-]{0,20}",op) else "other"
for tok,src in AIB.get("userFetchers",{}).items(): BOTS["ai_user_fetch"].setdefault(tok,slug(tok,src))
known=set().union(*[set(v) for v in BOTS.values()])
# Tokens are matched case-insensitively by the engine, so a bulk entry that differs from a curated
# one only in case would double-match and let JSON order decide the class. Keep the curated one.
known_ci={k.lower() for k in known}
BOTS["ai_crawler"].update({c:slug(c,"other") for c in AIB.get("crawlers",[]) if c.lower() not in known_ci})
BSRC={"ai_user_fetch":"https://developers.openai.com/api/docs/bots ; https://support.claude.com/en/articles/8896518 ; https://docs.perplexity.ai/docs/resources/perplexity-crawlers ; https://developers.google.com/crawling/docs/crawlers-fetchers/google-user-triggered-fetchers ; https://developers.facebook.com/docs/sharing/webmasters/web-crawlers/ ; https://github.com/ai-robots-txt/ai.robots.txt (MIT)",
 "ai_search_crawler":"https://developers.openai.com/api/docs/bots ; https://support.claude.com/en/articles/8896518 ; https://docs.perplexity.ai/docs/resources/perplexity-crawlers",
 "ai_crawler":"https://github.com/ai-robots-txt/ai.robots.txt (MIT) ; https://developers.cloudflare.com/ai-crawl-control/reference/bots/",
 "ad_verifier":"https://developers.openai.com/api/docs/bots ; https://developers.facebook.com/docs/sharing/webmasters/web-crawlers/",
 "link_preview":"https://developers.facebook.com/docs/sharing/webmasters/web-crawlers/ ; https://github.com/monperrus/crawler-user-agents (MIT)",
 "search_crawler":"https://developers.google.com/crawling/docs/crawlers-fetchers/overview-google-crawlers ; https://github.com/monperrus/crawler-user-agents (MIT)",
 "seo_tool":"https://github.com/monperrus/crawler-user-agents (MIT)","monitoring":"https://github.com/monperrus/crawler-user-agents (MIT)","automation":"https://github.com/omrilotan/isbot (Unlicense)"}
BVER={"ai_user_fetch":V,"ai_search_crawler":V,"ai_crawler":V,"ad_verifier":V,"link_preview":V,"search_crawler":None,"seo_tool":None,"monitoring":None,"automation":None}
R["botRules"]=[{"class":k,"token":t,"operator":s,"src":BSRC[k],"verified":BVER[k]} for k,v in BOTS.items() for t,s in v.items()]
R["botHeaders"]=[{"header":"signature-agent","contains":"chatgpt.com","class":"ai_user_fetch","operator":"chatgpt","src":"https://momenticmarketing.com/blog/ai-search-crawlers-bots (OpenAI Web Bot Auth, Signature-Agent \"https://chatgpt.com\")","verified":V},
 {"header":"sec-purpose","contains":"prefetch","class":"prefetch","operator":"browser","src":"https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Sec-Purpose","verified":V},
 {"header":"purpose","contains":"prefetch","class":"prefetch","operator":"browser","src":"https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Sec-Purpose","verified":V},
 {"header":"x-moz","contains":"prefetch","class":"prefetch","operator":"firefox","src":"https://developer.mozilla.org/en-US/docs/Glossary/Prefetch","verified":None}]
del R["bots"]; del R["aiUserFetchers"]
R["_provenance"]["v3"]="Rules annotated with src+verified in critic loop 2026-10-24 (America/Chicago). Only CC0/MIT/Apache/Unlicense-derived data shipped."
json.dump(R,open(os.path.join(OUT,"rules.json"),"w"),indent=1)
oss=json.load(open(os.path.join(HERE,"rules-oss.json")))
json.dump(oss,open(os.path.join(OUT,"oss-referrers.json"),"w"))
print(len(R["botRules"]))
