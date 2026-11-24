/** Static locale output from the existing HTML translations and Markdown sources. */
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';
import { parse, parseFragment, serialize } from 'parse5';
import { marked } from 'marked';

export const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
export const SITE = 'https://keyala.io';
export const LANGS = ['en', 'zh-Hant', 'ja'];
export const PAGES = ['home', 'guide', 'docs', 'privacy', 'support', 'news'];
const names = { en: 'English', 'zh-Hant': '繁體中文', ja: '日本語' };
const locale = { en: 'en_US', 'zh-Hant': 'zh_TW', ja: 'ja_JP' };
const seo = JSON.parse(fs.readFileSync(path.join(ROOT, 'site/seo.json')));
export const route = (page, lang) => `${lang === 'en' ? '/' : `/${lang}/`}${page === 'home' ? '' : `${page}/`}`;
const attr = (n, key) => n.attrs?.find(a => a.name === key)?.value;
function set(n, key, value) {
  n.attrs ||= [];
  const a = n.attrs.find(a => a.name === key);
  if (a) a.value = String(value); else n.attrs.push({ name: key, value: String(value) });
}
function remove(n) { const a = n.parentNode?.childNodes; if (a) a.splice(a.indexOf(n), 1); }
export function all(n, fn) {
  return [...(fn(n) ? [n] : []), ...(n.childNodes || []).flatMap(c => all(c, fn))];
}
const first = (n, fn) => all(n, fn)[0];
const tag = (n, t) => n.tagName === t;
export const text = n => n.nodeName === '#text' ? n.value : (n.childNodes || []).map(text).join('');
function content(n, html) {
  n.childNodes = parseFragment(html).childNodes;
  n.childNodes.forEach(c => c.parentNode = n);
}
const esc = s => String(s).replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;');
function append(n, html) {
  const children = parseFragment(html).childNodes;
  children.forEach(c => c.parentNode = n);
  n.childNodes.push(...children);
}
function strings(doc) {
  const script = first(doc, n => tag(n, 'script') && text(n).includes('window.KEYALA_STRINGS=')) ||
    first(doc, n => tag(n, 'script') && text(n).includes('window.KEYALA_STRINGS ='));
  if (!script) return {};
  const context = { window: {} };
  vm.runInNewContext(text(script), context, { timeout: 1000 });
  remove(script);
  return context.window.KEYALA_STRINGS;
}
function languageMenu(doc, lang, urls) {
  const menu = `<details class="language-picker"><summary lang="${lang}">${names[lang]}</summary><nav aria-label="Language">${Object.entries(urls).map(([l, u]) => `<a href="${esc(u)}" lang="${l}" hreflang="${l}" data-language-link${l === lang ? ' aria-current="true"' : ''}>${names[l]}</a>`).join('')}</nav></details>`;
  const picker = first(doc, n => attr(n, 'id') === 'language');
  if (picker) {
    const replacement = parseFragment(menu).childNodes[0];
    replacement.parentNode = picker.parentNode;
    picker.parentNode.childNodes.splice(picker.parentNode.childNodes.indexOf(picker), 1, replacement);
  } else {
    const nav = first(doc, n => tag(n, 'header'));
    if (nav) append(first(nav, n => attr(n, 'class')?.split(' ').includes('wrap')) || nav, menu);
  }
}
function links(doc, sourcePath, lang) {
  for (const n of all(doc, n => n.attrs)) for (const key of ['href', 'src', 'poster']) {
    const value = attr(n, key);
    if (!value || value.startsWith('#') || /^(mailto:|data:|tel:)/.test(value)) continue;
    const url = new URL(value, SITE + sourcePath);
    if (url.origin !== SITE) continue;
    const page = PAGES.find(p => url.pathname === route(p, 'en'));
    if (key === 'href' && page) url.pathname = route(page, lang);
    if (key === 'href' && url.pathname === '/docs/keyala.md') url.pathname = route('docs', lang) + 'keyala.md';
    set(n, key, url.pathname + url.search + url.hash);
  }
}
function metadata(doc, lang, title, desc, urls, canonical, article = false) {
  const head = first(doc, n => tag(n, 'head'));
  for (const n of all(head, n => tag(n, 'title') || (tag(n, 'meta') &&
    (['description', 'twitter:card', 'twitter:title', 'twitter:description', 'twitter:image'].includes(attr(n, 'name')) || attr(n, 'property')?.startsWith('og:'))) ||
    (tag(n, 'link') && (attr(n, 'rel') === 'canonical' || attr(n, 'hreflang'))))) remove(n);
  append(head, `<title>${esc(title)}</title><meta name="description" content="${esc(desc)}">
<link rel="canonical" href="${canonical}">
${Object.entries(urls).map(([l, u]) => `<link rel="alternate" hreflang="${l}" href="${SITE}${u}">`).join('')}
<link rel="alternate" hreflang="x-default" href="${SITE}${urls.en || Object.values(urls)[0]}">
<meta property="og:site_name" content="Keyala"><meta property="og:type" content="${article ? 'article' : 'website'}">
<meta property="og:title" content="${esc(title)}"><meta property="og:description" content="${esc(desc)}"><meta property="og:url" content="${canonical}">
<meta property="og:locale" content="${locale[lang]}"><meta property="og:image" content="${SITE}/social.svg">
<meta name="twitter:card" content="summary_large_image"><meta name="twitter:title" content="${esc(title)}"><meta name="twitter:description" content="${esc(desc)}"><meta name="twitter:image" content="${SITE}/social.svg">`);
}
function documentPage(doc, lang) {
  const ui = seo[lang].docsUI;
  const md = fs.readFileSync(path.join(ROOT, `site/docs/keyala${lang === 'en' ? '' : `.${lang}`}.md`), 'utf8');
  const article = first(doc, n => attr(n, 'id') === 'doc');
  content(article, marked.parse(md));
  const used = new Set();
  const toc = first(doc, n => attr(n, 'id') === 'toc');
  const english = marked.lexer(fs.readFileSync(path.join(ROOT, 'site/docs/keyala.md'), 'utf8')).filter(t => t.type === 'heading' && [2, 3].includes(t.depth));
  for (const [i, h] of all(article, n => ['h2', 'h3'].includes(n.tagName)).entries()) {
    // Stable English-derived IDs let translated pages preserve shared section links.
    let id = english[i].text.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'section';
    while (used.has(id)) id += '-2';
    used.add(id); set(h, 'id', id);
    const label = text(h);
    append(toc, `<a class="${h.tagName}" href="#${id}">${esc(label)}</a>`);
    append(h, ` <a class="anchor" href="#${id}" aria-label="${ui[7]}">#</a>`);
  }
  set(toc, 'aria-label', ui[0]);
  const summary = first(doc, n => tag(n, 'summary'));
  content(summary, ui[0]);
  const copy = first(doc, n => attr(n, 'id') === 'copy-md');
  content(copy, ui[2]);
  const download = first(doc, n => attr(n, 'download'));
  content(download, ui[1]);
  const note = first(doc, n => attr(n, 'class') === 'ai-note');
  content(note, `${ui[3]} <a href="keyala.md">keyala.md</a>`);
  for (const n of all(doc, n => tag(n, 'noscript') || (tag(n, 'script') && attr(n, 'src')?.includes('marked')))) remove(n);
  set(copy, 'data-copy-label', ui[4]); set(copy, 'data-copied-label', ui[5]); set(copy, 'data-error-label', ui[6]);
  return md;
}
export function render(source, page, lang, sourcePath, urls) {
  const doc = parse(source), translations = strings(doc);
  const T = translations[lang] || {}, EN = translations.en || {};
  for (const n of all(doc, n => attr(n, 'data-language') !== undefined)) {
    if (attr(n, 'data-language') !== lang) remove(n); else n.attrs = n.attrs.filter(a => a.name !== 'hidden');
  }
  for (const [key, target] of [['data-i18n', null], ['data-i18n-html', 'html'], ['data-i18n-alt', 'alt'], ['data-i18n-href', 'href']]) {
    for (const n of all(doc, n => attr(n, key) !== undefined)) {
      const value = T[attr(n, key)] ?? EN[attr(n, key)];
      if (value === undefined) throw Error(`Missing ${lang} translation: ${attr(n, key)}`);
      if (target && target !== 'html') set(n, target, value); else content(n, target === 'html' ? value : esc(value));
    }
  }
  const html = first(doc, n => tag(n, 'html')); set(html, 'lang', lang); set(html, 'data-static-lang', lang);
  let md;
  if (page === 'docs') md = documentPage(doc, lang);
  if (page === 'news') {
    for (const li of all(doc, n => tag(n, 'li') && attr(n, 'data-cat') !== undefined)) {
      const variants = all(li, n => attr(n, 'data-lang') !== undefined);
      const pick = variants.find(n => attr(n, 'data-lang') === lang) || variants.find(n => attr(n, 'data-lang') === 'en') || variants[0];
      for (const n of variants) if (n !== pick) remove(n);
      for (const n of all(pick, n => attr(n, 'class') === 'tag')) content(n, seo[lang].newsUI[attr(li, 'data-cat')]);
    }
    for (const n of all(doc, n => attr(n, 'data-ui') !== undefined)) content(n, seo[lang].newsUI[attr(n, 'data-ui')]);
    for (const n of all(doc, n => tag(n, 'script') && !attr(n, 'src'))) remove(n);
    append(first(doc, n => tag(n, 'body')), '<script src="/news.js" defer></script>');
    const empty = first(doc, n => attr(n, 'class')?.includes('empty'));
    if (!all(doc, n => tag(n, 'li') && attr(n, 'data-cat') !== undefined).length) empty.attrs = empty.attrs.filter(a => a.name !== 'hidden');
  }
  // Article language is fixed by its existing URL; remove the old picker listener.
  if (page === 'article') for (const n of all(doc, n => tag(n, 'script') && !attr(n, 'src'))) remove(n);
  for (const n of all(doc, n => tag(n, 'script') && attr(n, 'src')?.endsWith('i18n.js'))) remove(n);
  for (const n of all(doc, n => tag(n, 'script') && !attr(n, 'src'))) {
    for (const c of n.childNodes) if (c.value) c.value = c.value.replaceAll("'media/keyala-launch-4x5.jpg'", "'/media/keyala-launch-4x5.jpg'");
  }
  // Docs uses plain nav text instead of translation attributes.
  const nav = first(doc, n => tag(n, 'header'));
  const targets = ['docs', 'guide', 'news'];
  if (page === 'docs') {
    for (const a of all(nav, n => tag(n, 'a'))) {
      const href = attr(a, 'href');
      const target = targets.find(p => new URL(href, SITE + sourcePath).pathname === route(p, 'en'));
      if (target) content(a, seo[lang].nav[targets.indexOf(target)]);
      if (href?.includes('app.keyala.io/console')) content(a, seo[lang].nav[3]);
    }
    const foot = first(doc, n => tag(n, 'footer'));
    content(first(foot, n => tag(n, 'span')), {en:'Keyala is an experimental beta.', 'zh-Hant':'Keyala 是實驗性 beta。',ja:'Keyala は実験的なベータ版です。'}[lang]);
    for (const a of all(foot, n => tag(n, 'a'))) content(a, seo[lang].nav[attr(a, 'href').includes('privacy') ? 4 : 5]);
  }
  const skip = first(doc, n => attr(n, 'class') === 'skip');
  if (skip) content(skip, {en:'Skip to content', 'zh-Hant':'跳到內容', ja:'本文へ移動'}[lang]);
  if (page === 'home') {
    const receipt = first(doc, n => attr(n, 'class') === 'receipt');
    if (receipt) set(receipt, 'aria-label', {en:'Example receipt: Interview Prepper, key not shared with the app, provider cost $0.0169, Keyala fee $0.00', 'zh-Hant':'範例收據：Interview Prepper，金鑰未交給應用，服務商費用 $0.0169，Keyala 費用 $0.00', ja:'利用明細の例：Interview Prepper、アプリへのキー共有なし、プロバイダー費用 $0.0169、Keyala 手数料 $0.00'}[lang]);
  }
  links(doc, sourcePath, lang);
  let title, desc;
  if (page === 'article') {
    title = text(first(doc, n => tag(n, 'title')));
    desc = attr(first(doc, n => tag(n, 'meta') && attr(n, 'name') === 'description'), 'content');
  } else [title, desc] = seo[lang][page];
  languageMenu(doc, lang, urls);
  metadata(doc, lang, title, desc, urls, SITE + urls[lang], page === 'article');
  append(first(doc, n => tag(n, 'body')), '<script src="/locale.js" defer></script>');
  return { html: serialize(doc), md };
}
function write(out, url, html) {
  const dir = path.join(out, url); fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'index.html'), html);
}
export function build(out) {
  const groups = [];
  for (const page of PAGES) {
    const urls = Object.fromEntries(LANGS.map(l => [l, route(page, l)]));
    const sourcePath = route(page, 'en');
    const source = fs.readFileSync(path.join(page === 'news' ? out : path.join(ROOT, 'site'), sourcePath, 'index.html'), 'utf8');
    for (const lang of LANGS) {
      const result = render(source, page, lang, sourcePath, urls);
      write(out, urls[lang], result.html);
      if (result.md) fs.writeFileSync(path.join(out, urls[lang], 'keyala.md'), result.md);
    }
    if (!['privacy', 'support'].includes(page)) groups.push(urls);
  }
  const news = path.join(out, 'news');
  for (const stem of fs.readdirSync(news, { withFileTypes: true }).filter(d => d.isDirectory() && LANGS.some(l => fs.existsSync(path.join(news, d.name, l, 'index.html'))))) {
    const urls = Object.fromEntries(LANGS.filter(l => fs.existsSync(path.join(news, stem.name, l, 'index.html'))).map(l => [l, `/news/${stem.name}/${l}/`]));
    for (const [lang, url] of Object.entries(urls)) {
      const file = path.join(out, url, 'index.html');
      const result = render(fs.readFileSync(file, 'utf8'), 'article', lang, url, urls);
      write(out, url, result.html);
    }
    groups.push(urls);
  }
  const sitemap = groups.flatMap(urls => Object.values(urls).map(url => `<url><loc>${SITE}${url}</loc>${Object.entries({...urls, 'x-default': urls.en || Object.values(urls)[0]}).map(([l,u]) => `<xhtml:link rel="alternate" hreflang="${l}" href="${SITE}${u}"/>`).join('')}</url>`)).join('');
  fs.writeFileSync(path.join(out, 'sitemap.xml'), `<?xml version="1.0" encoding="UTF-8"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml">${sitemap}</urlset>`);
  // Translation sources are downloadable via their own locale's keyala.md only.
  for (const lang of ['zh-Hant', 'ja']) fs.rmSync(path.join(out, `docs/keyala.${lang}.md`), {force:true});
  fs.rmSync(path.join(out, 'seo.json'), {force:true});
  console.log(`site: ${PAGES.length * LANGS.length} static locale pages plus translated news articles -> ${out}`);
}
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  if (!process.argv[2]) throw Error('Usage: node tools/site/build.mjs OUT_DIR (after build_news.py)');
  build(path.resolve(process.argv[2]));
}
