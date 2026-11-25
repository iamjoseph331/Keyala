#!/usr/bin/env python3
"""Build keyala.io/news from site/news/posts/*.md (Python stdlib only, no build dependencies).

Usage: python3 tools/build_news.py OUT_DIR      (Pages: OUT_DIR = _site; preview: any folder)
Post file: site/news/posts/<stem>.<lang>.md, lang = en | zh-Hant | ja. Files sharing a stem are
translations of one post. Front matter (between --- lines): title, date (YYYY-MM-DD),
category (news | tech), summary, optional image (path relative to the site root).
Writes OUT/news/index.html, OUT/news/<stem>/<lang>/index.html, OUT/news/<stem>/index.html
(redirects to the English or first version) and OUT/news/feed.xml.
"""
import html, json, re, sys
from datetime import date
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
POSTS = ROOT / "site" / "news" / "posts"
SITE = "https://keyala.io"
LANGS = ["en", "zh-Hant", "ja"]
CATS = {"news": {"en": "News", "zh-Hant": "最新消息", "ja": "ニュース"},
        "tech": {"en": "Tech blog", "zh-Hant": "技術部落格", "ja": "技術ブログ"}}
UI = {
    "en": {"title": "News", "lead": "Updates from Keyala and notes on how it is built.", "all": "All", "read": "Read",
           "back": "All news", "also": "Also in", "feed": "RSS feed", "none": "No posts here yet."},
    "zh-Hant": {"title": "最新消息", "lead": "Keyala 的更新，以及我們如何打造它的筆記。", "all": "全部", "read": "閱讀",
                "back": "所有消息", "also": "其他語言", "feed": "RSS 訂閱", "none": "這裡還沒有文章。"},
    "ja": {"title": "ニュース", "lead": "Keyala のお知らせと、つくり方についてのメモ。", "all": "すべて", "read": "読む",
           "back": "ニュース一覧", "also": "ほかの言語", "feed": "RSS フィード", "none": "まだ記事がありません。"},
}
LANG_NAME = {"en": "English", "zh-Hant": "繁體中文", "ja": "日本語"}
NAV = {  # shared nav/footer strings, same keys as the other pages
    "en": {"nav.dev": "Developers", "nav.users": "Users", "nav.news": "News", "nav.console": "Console",
           "ft.priv": "Privacy", "ft.sup": "Support", "ft.by": "Experimental personal project by Joseph Chen"},
    "zh-Hant": {"nav.dev": "開發者", "nav.users": "使用者", "nav.news": "消息", "nav.console": "控制台",
                "ft.priv": "隱私", "ft.sup": "支援", "ft.by": "Joseph Chen 個人營運的實驗性專案"},
    "ja": {"nav.dev": "開発者", "nav.users": "ユーザー", "nav.news": "ニュース", "nav.console": "コンソール",
           "ft.priv": "プライバシー", "ft.sup": "サポート", "ft.by": "Joseph Chen による個人の実験的プロジェクト"},
}

# ---------- Markdown (the subset our posts use) ----------
# ponytail: headings, paragraphs, flat lists, fences, quotes, images, links, code, bold/italic.
# No tables or nested lists; switch to a real Markdown library if posts need them.
def inline(t):
    t = html.escape(t, quote=False)
    codes = []
    t = re.sub(r"`([^`]+)`", lambda m: codes.append(m.group(1)) or f"\0{len(codes) - 1}\0", t)
    t = re.sub(r"!\[([^\]]*)\]\(([^)\s]+)\)", lambda m: f'<img src="{attr(m.group(2))}" alt="{m.group(1)}" loading="lazy">', t)
    t = re.sub(r"\[([^\]]+)\]\(([^)\s]+)\)", lambda m: f'<a href="{attr(m.group(2))}">{m.group(1)}</a>', t)
    t = re.sub(r"\*\*(.+?)\*\*", r"<strong>\1</strong>", t)
    t = re.sub(r"(?<![\w*])\*(?!\s)(.+?)(?<!\s)\*(?![\w*])", r"<em>\1</em>", t)
    return re.sub(r"\0(\d+)\0", lambda m: f"<code>{codes[int(m.group(1))]}</code>", t)

def attr(url):
    url = html.unescape(url)
    if re.match(r"(?i)\s*(javascript|data|vbscript):", url):
        url = "#"
    return html.escape(url, quote=True)

def markdown(src):
    out, para, lines, i = [], [], src.split("\n"), 0
    def flush():
        if para:
            out.append("<p>" + inline(" ".join(para)) + "</p>"); para.clear()
    while i < len(lines):
        line = lines[i]
        if line.startswith("```"):
            flush(); lang = line[3:].strip(); body = []; i += 1
            while i < len(lines) and not lines[i].startswith("```"):
                body.append(lines[i]); i += 1
            cls = f' class="language-{html.escape(lang)}"' if lang else ""
            out.append(f"<pre><code{cls}>{html.escape(chr(10).join(body))}</code></pre>")
        elif m := re.match(r"(#{1,4})\s+(.*)", line):
            flush(); n = min(len(m.group(1)) + 1, 4)  # the post title is the page's h1
            out.append(f"<h{n} id=\"{slug(m.group(2))}\">{inline(m.group(2))}</h{n}>")
        elif re.match(r"\s*([-*]|\d+\.)\s+", line):
            flush(); tag = "ol" if re.match(r"\s*\d+\.", line) else "ul"; items = []
            while i < len(lines) and re.match(r"\s*([-*]|\d+\.)\s+", lines[i]):
                items.append("<li>" + inline(re.sub(r"\s*([-*]|\d+\.)\s+", "", lines[i], count=1)) + "</li>"); i += 1
            out.append(f"<{tag}>{''.join(items)}</{tag}>"); continue
        elif line.startswith(">"):
            flush(); quote = []
            while i < len(lines) and lines[i].startswith(">"):
                quote.append(lines[i][1:].strip()); i += 1
            out.append("<blockquote><p>" + inline(" ".join(quote)) + "</p></blockquote>"); continue
        elif not line.strip():
            flush()
        else:
            para.append(line.strip())
        i += 1
    flush()
    return "\n".join(out)

def slug(t):
    return re.sub(r"[^\w-]+", "-", t.lower(), flags=re.U).strip("-")[:60] or "s"

# ---------- posts ----------
def load():
    posts = []
    for f in sorted(POSTS.glob("*.md")):
        m = re.match(r"(.+)\.(en|zh-Hant|ja)\.md$", f.name)
        if not m:
            sys.exit(f"{f.name}: name must be <stem>.<en|zh-Hant|ja>.md")
        raw = f.read_text(encoding="utf-8")
        fm = re.match(r"---\n(.*?)\n---\n(.*)", raw, re.S)
        if not fm:
            sys.exit(f"{f.name}: missing --- front matter ---")
        meta = {k: v.strip().strip('"') for k, v in (re.match(r"(\w+):\s*(.*)", l).groups() for l in fm.group(1).splitlines() if l.strip())}
        for k in ("title", "date", "category", "summary"):
            if not meta.get(k):
                sys.exit(f"{f.name}: front matter needs {k}")
        if meta["category"] not in CATS:
            sys.exit(f"{f.name}: category must be one of {', '.join(CATS)}")
        date.fromisoformat(meta["date"])
        posts.append({**meta, "stem": m.group(1), "lang": m.group(2), "body": markdown(fm.group(2))})
    return posts

def page(lang, title, desc, url, body, depth, extra_head=""):
    up = "../" * depth
    strings = {l: NAV[l] for l in LANGS}
    return f"""<!doctype html>
<html lang="{lang}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>{html.escape(title)} - Keyala</title><meta name="description" content="{html.escape(desc)}">
<link rel="canonical" href="{url}"><meta property="og:title" content="{html.escape(title)}">
<meta property="og:description" content="{html.escape(desc)}"><meta property="og:url" content="{url}">
<meta property="og:image" content="{SITE}/media/social/keyala-cover-a-{lang}-v1.png"><meta property="og:type" content="article">
<link rel="alternate" type="application/rss+xml" title="Keyala news" href="{SITE}/news/feed.xml">
<link rel="icon" href="{up}brand/koala.svg"><link rel="stylesheet" href="{up}base.css"><link rel="stylesheet" href="{up}news.css">
{extra_head}<script>window.KEYALA_STRINGS={json.dumps(strings, ensure_ascii=False)};</script><script src="{up}i18n.js" defer></script></head>
<body><a class="skip" href="#main">Skip to content</a>
<header class="nav"><div class="wrap"><a class="brand" href="{up}"><img src="{up}brand/koala.svg" alt=""> Keyala</a><a class="link" href="{up}docs/" data-i18n="nav.dev">Developers</a><a class="link" href="{up}guide/" data-i18n="nav.users">Users</a><a class="link" href="{up}news/" aria-current="page" data-i18n="nav.news">News</a><a class="link" href="https://app.keyala.io/console" data-i18n="nav.console">Console</a><select id="language" aria-label="Language"><option value="en">English</option><option value="zh-Hant">繁體中文</option><option value="ja">日本語</option></select></div></header>
<main id="main" class="wrap news">{body}</main>
<footer class="footer"><div class="wrap"><a href="{up}privacy/" data-i18n="ft.priv">Privacy</a><a href="{up}support/" data-i18n="ft.sup">Support</a><a href="{up}news/feed.xml">RSS</a><a href="https://github.com/iamjoseph331/Keyala">GitHub</a><a href="mailto:support@keyala.io">support@keyala.io</a><span data-i18n="ft.by">Experimental personal project by Joseph Chen</span></div></footer>
</body></html>
"""

def build(out):
    posts = load()
    news = out / "news"
    groups = {}
    for p in posts:
        groups.setdefault(p["stem"], {})[p["lang"]] = p
    order = sorted(groups, key=lambda s: max(p["date"] for p in groups[s].values()), reverse=True)

    for stem, by_lang in groups.items():
        for lang, p in by_lang.items():
            others = "".join(f'<a href="../{l}/" hreflang="{l}" data-lang="{l}">{LANG_NAME[l]}</a>'
                             for l in LANGS if l in by_lang and l != lang)
            alt = "".join(f'<link rel="alternate" hreflang="{l}" href="{SITE}/news/{stem}/{l}/">' for l in by_lang)
            img = f'<img class="cover" src="../../../{attr(p["image"])}" alt="">' if p.get("image") else ""
            body = (f'<article class="post" lang="{lang}"><p class="crumbs"><a href="../../">&larr; {UI[lang]["back"]}</a></p>'
                    f'<p class="meta"><span class="tag">{CATS[p["category"]][lang]}</span> <time datetime="{p["date"]}">{p["date"]}</time></p>'
                    f'<h1>{html.escape(p["title"])}</h1><p class="lead">{html.escape(p["summary"])}</p>{img}'
                    f'<div class="prose">{p["body"]}</div>'
                    + (f'<p class="also">{UI[lang]["also"]}: {others}</p>' if others else "") + "</article>"
                    # the language picker on a post jumps to that translation, or to the list
                    + f'<script>document.getElementById("language").addEventListener("change",e=>{{const l=e.target.value;'
                      f'location.href={json.dumps(list(by_lang))}.includes(l)?"../"+l+"/":"../../?lang="+l}});'
                      f'document.documentElement.lang={json.dumps(lang)};</script>')
            d = news / stem / lang
            d.mkdir(parents=True, exist_ok=True)
            (d / "index.html").write_text(page(lang, p["title"], p["summary"], f"{SITE}/news/{stem}/{lang}/", body, 3, alt), encoding="utf-8")
        first = "en" if "en" in by_lang else next(l for l in LANGS if l in by_lang)
        (news / stem / "index.html").write_text(
            f'<!doctype html><meta charset="utf-8"><meta http-equiv="refresh" content="0; url={first}/">'
            f'<link rel="canonical" href="{SITE}/news/{stem}/{first}/"><a href="{first}/">{html.escape(by_lang[first]["title"])}</a>', encoding="utf-8")

    # list: one card per post, all translations rendered, script shows the best one for the UI language
    cards = []
    for stem in order:
        variants = "".join(
            f'<a class="card item" data-lang="{l}" href="{stem}/{l}/" lang="{l}"><p class="meta"><span class="tag">{CATS[p["category"]][l]}</span> '
            f'<time datetime="{p["date"]}">{p["date"]}</time></p><h2>{html.escape(p["title"])}</h2><p class="muted">{html.escape(p["summary"])}</p></a>'
            for l, p in groups[stem].items())
        cat = next(iter(groups[stem].values()))["category"]
        cards.append(f'<li data-cat="{cat}">{variants}</li>')
    tabs = "".join(f'<button type="button" data-cat="{c}" data-ui="{c}">{CATS[c]["en"]}</button>' for c in CATS)
    body = (f'<h1 data-ui="title">News</h1><p class="lead" data-ui="lead">{UI["en"]["lead"]}</p>'
            f'<div class="filters" role="group"><button type="button" data-cat="" data-ui="all" aria-pressed="true">All</button>{tabs}</div>'
            f'<ul class="list">{"".join(cards)}</ul><p class="muted empty" data-ui="none" hidden>{UI["en"]["none"]}</p>'
            f'<p><a href="feed.xml" data-ui="feed">RSS feed</a></p>'
            f'<script>const UI={json.dumps(UI, ensure_ascii=False)},CATS={json.dumps(CATS, ensure_ascii=False)};let cat=new URLSearchParams(location.search).get("c")||"";'
            'function draw(){const l=document.documentElement.lang,T=UI[l]||UI.en;'
            'document.querySelectorAll("[data-ui]").forEach(e=>{const k=e.dataset.ui;e.textContent=CATS[k]?CATS[k][l]||CATS[k].en:T[k]});'
            'let shown=0;document.querySelectorAll(".list>li").forEach(li=>{const v=[...li.children],pick=v.find(a=>a.dataset.lang===l)||v.find(a=>a.dataset.lang==="en")||v[0];'
            'v.forEach(a=>a.hidden=a!==pick);li.hidden=!!cat&&li.dataset.cat!==cat;if(!li.hidden)shown++});'
            'document.querySelector(".empty").hidden=shown>0;'
            'document.querySelectorAll(".filters button").forEach(b=>b.setAttribute("aria-pressed",String(b.dataset.cat===cat)))}'
            'document.querySelectorAll(".filters button").forEach(b=>b.onclick=()=>{cat=b.dataset.cat;history.replaceState(null,"",cat?"?c="+cat:location.pathname);draw()});'
            'document.addEventListener("keyala:lang",draw);</script>')
    news.mkdir(parents=True, exist_ok=True)
    (news / "index.html").write_text(page("en", "News", UI["en"]["lead"], f"{SITE}/news/", body, 1), encoding="utf-8")

    items = "".join(
        f"<item><title>{html.escape(p['title'])}</title><link>{SITE}/news/{p['stem']}/{p['lang']}/</link>"
        f"<guid>{SITE}/news/{p['stem']}/{p['lang']}/</guid><category>{p['category']}</category>"
        f"<pubDate>{date.fromisoformat(p['date']).strftime('%a, %d %b %Y 00:00:00 +0000')}</pubDate>"
        f"<description>{html.escape(p['summary'])}</description></item>"
        for p in sorted(posts, key=lambda p: p["date"], reverse=True))
    (news / "feed.xml").write_text(
        f'<?xml version="1.0" encoding="utf-8"?><rss version="2.0"><channel><title>Keyala news</title>'
        f"<link>{SITE}/news/</link><description>{UI['en']['lead']}</description>{items}</channel></rss>", encoding="utf-8")
    # sitemap: indexable static pages (privacy/support/review/tutorials are noindex) + every post translation
    locs = [f"{SITE}/{x}" for x in ("", "guide/", "docs/", "developers/", "news/")]
    locs += [f"{SITE}/news/{p['stem']}/{p['lang']}/" for p in posts]
    (out / "sitemap.xml").write_text(
        '<?xml version="1.0" encoding="utf-8"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">'
        + "".join(f"<url><loc>{u}</loc></url>" for u in locs) + "</urlset>", encoding="utf-8")
    print(f"news: {len(groups)} posts, {len(posts)} pages -> {news}")

def selftest():
    h = markdown("# T\n\nA *b* **c** `<x>` [l](https://k.io) [x](javascript:alert(1))\n\n- one\n- two\n\n```js\na<b\n```\n\n> q")
    assert '<h2 id="t">T</h2>' in h and "<em>b</em>" in h and "<strong>c</strong>" in h, h
    assert "<code>&lt;x&gt;</code>" in h and '<a href="https://k.io">l</a>' in h and '<a href="#">x</a>' in h, h
    assert "<ul><li>one</li><li>two</li></ul>" in h and '<code class="language-js">a&lt;b</code>' in h, h
    assert "<blockquote><p>q</p></blockquote>" in h and "<script" not in markdown("<script>x</script>"), h
    print("selftest ok")

if __name__ == "__main__":
    if sys.argv[1:] == ["--test"]:
        sys.exit(selftest())
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    build(Path(sys.argv[1]).resolve())
