import yaml,json,csv,re,os
S='src/'
out={}
# referrers
ref={}
sp=yaml.safe_load(open(S+'snowplow-referer-parser__referer-parser/resources/referers.yml'))
for med,srcs in sp.items():
  for name,v in srcs.items():
    for d in v.get('domains',[]): ref.setdefault(d.lower(),{'source':name,'medium':med,'from':['snowplow']})
M=S+'matomo-org__searchengine-and-social-list/'
for f,med in [('SearchEngines.yml','search'),('Socials.yml','social'),('AIAssistants.yml','ai_assistant')]:
  y=yaml.safe_load(open(M+f))
  for name,v in y.items():
    urls=v if isinstance(v,list) else []
    if isinstance(v,list) and v and isinstance(v[0],dict):
      urls=[u for e in v for u in e.get('urls',[])]
    for d in urls:
      d=str(d).lower(); e=ref.setdefault(d,{'source':name,'medium':med,'from':[]})
      if med=='ai_assistant': e['medium']='ai_assistant'; e['source']=name
      e['from'].append('matomo')
for d,n in json.load(open(S+'plausible__analytics/priv/custom_sources.json')).items():
  e=ref.setdefault(d.lower(),{'source':n,'medium':None,'from':[]}); e['from'].append('plausible')
ga4={}
for row in csv.reader(open(S+'plausible__analytics/priv/ga4-source-categories.csv')):
  if len(row)==2 and row[1].startswith('SOURCE'): ga4[row[0].lower()]=row[1].replace('SOURCE_CATEGORY_','').lower()
out['referrers']=dict(sorted(ref.items()))
out['ga4_source_categories']=ga4
out['click_ids']={
 'gclid':'Google Ads','gclsrc':'Google Ads/SA360','dclid':'Google Display/CM360','gbraid':'Google Ads iOS app->web','wbraid':'Google Ads web->app','gad_source':'Google Ads (auto)','gad_campaignid':'Google Ads',
 'srsltid':'Google Merchant Center (ORGANIC shopping, not paid!)',
 'msclkid':'Microsoft Ads','fbclid':'Meta (organic AND paid; not paid signal alone)','igshid':'Instagram share (organic)','igsh':'Instagram share (organic)',
 'ttclid':'TikTok Ads','twclid':'X/Twitter Ads','li_fat_id':'LinkedIn Ads','rdt_cid':'Reddit Ads','epik':'Pinterest Ads','sccid':'Snapchat Ads','ScCid':'Snapchat Ads','qclid':'Quora Ads',
 'yclid':'Yandex Direct','ysclid':'Yandex (organic tracking)','obclid':'Outbrain','ob_click_id':'Outbrain','tblci':'Taboola','irclickid':'Impact','irclid':'Impact','_kx':'Klaviyo email','mc_cid':'Mailchimp campaign','mc_eid':'Mailchimp subscriber',
 '_hsenc':'HubSpot email','_hsmi':'HubSpot email','mkt_tok':'Marketo email','vero_id':'Vero email','oly_enc_id':'Omeda email','s_kwcid':'Adobe/AMO search','ef_id':'Adobe AMO','cjevent':'CJ affiliate','awc':'Awin affiliate','ranMID':'Rakuten affiliate','ranSiteID':'Rakuten affiliate',
 'oppref':'OpenAI ads','utm_source=chatgpt.com':'ChatGPT search auto-tag','si':'YouTube/Spotify share','ref_src':'Twitter legacy','s':'X share (s=20 etc)','t':'X share',
 'wickedid':'Wicked Reports','bfcid':'Freebuff'}
# ClearURLs param names
cu=json.load(open(S+'ClearURLs__Rules/data.min.json'))
params=set()
for p,v in cu['providers'].items():
  for r in v.get('rules',[])+v.get('referralMarketing',[]): params.add(r)
out['clearurls_tracking_param_patterns']=sorted(params)
# AI bots
rb=json.load(open(S+'ai-robots-txt__ai.robots.txt/robots.json'))
out['ai_bots']={k:{x:v.get(x) for x in ('operator','function','frequency')} for k,v in rb.items()}
out['ai_referrers']={d:e['source'] for d,e in ref.items() if e['medium']=='ai_assistant'}
out['ai_referrers'].update({d:n for d,n in {'chatgpt.com':'ChatGPT','perplexity.ai':'Perplexity','claude.ai':'Claude','gemini.google.com':'Gemini','copilot.microsoft.com':'Copilot','grok.com':'Grok','chat.deepseek.com':'DeepSeek','chat.mistral.ai':'Mistral','meta.ai':'Meta AI','you.com':'You.com','phind.com':'Phind','kagi.com/assistant':'Kagi','poe.com':'Poe','duck.ai':'Duck.ai','search.brave.com/ask':'Brave Leo','notebooklm.google.com':'NotebookLM','huggingface.co/chat':'HuggingChat','chat.qwen.ai':'Qwen','kimi.com':'Kimi','doubao.com':'Doubao'}.items()})
out['inapp_ua_tokens']={
 'Facebook':r'FBAN|FBAV|FB_IAB|FBIOS|FB4A|\[FB','Messenger':r'FBAN/Messenger|MessengerForiOS|Orca-Android','Instagram':r'\bInstagram ','Threads':r'Barcelona','WhatsApp':r'WAiOS/|WA4A/|WhatsApp','TikTok':r'musical_ly|BytedanceWebview|TikTok|trill_|ByteLocale','Douyin':r'aweme','Snapchat':r'Snapchat','LinkedIn':r'LinkedInApp','Twitter/X':r'Twitter for|TwitterAndroid','Pinterest':r'\[Pinterest/|Pinterest','Reddit':r'Reddit/|RedditAndroid','Line':r'\bLine/','WeChat':r'MicroMessenger','QQ':r'QQ/|MQQBrowser','Weibo':r'Weibo','KakaoTalk':r'KAKAOTALK','Naver':r'NAVER\(inapp','Telegram':r'Telegram(-Android)?/','Discord':r'Discord','Google app (GSA)':r'GSA/','Bing app':r'BingSapphire|BingWeb','Yahoo Japan':r'YJApp','Zalo':r'Zalo','Teams':r'Teams/','Slack':r'Slack(_SSB)?/','Android WebView generic':r'; wv\)','iOS WKWebView generic':r'Mobile/\w+$ (no Safari/ token)','IABMV':r'IABMV/1 (Meta in-app browser marker)'}
os.makedirs('data',exist_ok=True)
json.dump(out,open('data/merged.json','w'),indent=1)
yaml.safe_dump(out,open('data/merged.yaml','w'),allow_unicode=True,sort_keys=False)
print({k:len(v) for k,v in out.items()})
