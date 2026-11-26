import test, { after } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { parse } from 'parse5';
import { ROOT, SITE, LANGS, PAGES, NOINDEX_PAGES, route, build, all, text } from './build.mjs';

const out = fs.mkdtempSync(path.join(os.tmpdir(), 'keyala-site-test-'));
fs.cpSync(path.join(ROOT, 'site'), out, {recursive:true});
for (const app of ['resume-tailor', 'flashcards', 'daily-reflection']) {
  fs.cpSync(path.join(ROOT, 'examples', app), path.join(out, 'examples', app), { recursive: true });
}
execFileSync('python3', [path.join(ROOT, 'tools/build_news.py'), out]);
build(out);
after(() => fs.rmSync(out, {recursive:true, force:true}));
const attr = (n, k) => n.attrs?.find(a => a.name === k)?.value;
const read = url => parse(fs.readFileSync(path.join(out, url, 'index.html'), 'utf8'));
const one = (doc, fn) => { const nodes=all(doc,fn); assert.equal(nodes.length,1); return nodes[0]; };
const meta = (doc, name) => attr(one(doc,n => n.tagName === 'meta' && attr(n,'name')===name),'content');
const socialAlt = JSON.parse(fs.readFileSync(path.join(ROOT,'site/seo.json'),'utf8'));
const codeBlocks = s => [...s.matchAll(/```[^\n]*\n[\s\S]*?```/g)].map(m=>m[0]);

test('every locale has static content, matching metadata and reciprocal language URLs', () => {
  for (const page of PAGES) for (const lang of LANGS) {
    const url=route(page,lang), doc=read(url);
    assert.equal(attr(one(doc,n=>n.tagName==='html'),'lang'),lang);
    const title=text(one(doc,n=>n.tagName==='title'));
    assert(title.includes('Keyala'));
    assert(meta(doc,'description').length>20);
    assert.equal(meta(doc,'twitter:title'),title);
    assert.equal(meta(doc,'twitter:image'),`${SITE}/media/social/keyala-cover-a-${lang}-v1.png`);
    assert.equal(meta(doc,'twitter:image:alt'),socialAlt[lang].socialAlt);
    for(const [key,value] of Object.entries({'og:image':meta(doc,'twitter:image'),'og:image:type':'image/png','og:image:width':'1200','og:image:height':'630','og:image:alt':socialAlt[lang].socialAlt})) assert.equal(attr(one(doc,n=>n.tagName==='meta' && attr(n,'property')===key),'content'),value);
    assert.equal(attr(one(doc,n=>n.tagName==='meta' && attr(n,'property')==='og:title'),'content'),title);
    assert.equal(attr(one(doc,n=>n.tagName==='link' && attr(n,'rel')==='canonical'),'href'),SITE+url);
    for(const l of [...LANGS,'x-default']) assert.equal(attr(one(doc,n=>n.tagName==='link' && attr(n,'hreflang')===l),'href'),SITE+route(page,l==='x-default'?'en':l));
    assert.equal(all(doc,n=>attr(n,'data-language') && attr(n,'data-language')!==lang).length,0);
    assert(!text(doc).includes('window.KEYALA_STRINGS'));
    assert(!all(doc,n=>n.tagName==='script').some(n=>attr(n,'src')?.includes('i18n.js') || attr(n,'src')?.includes('cdnjs')));
    if(NOINDEX_PAGES.includes(page)) assert.equal(meta(doc,'robots'),'noindex');
    else assert.equal(all(doc,n=>n.tagName==='meta' && attr(n,'name')==='robots' && attr(n,'content').includes('noindex')).length,0);
    assert.equal(all(doc,n=>n.tagName==='a' && attr(n,'data-language-link')!==undefined).length,3);
    if(page==='docs') {
      assert.equal(all(doc,n=>n.tagName==='h2').length,8);
      assert.equal(all(doc,n=>n.tagName==='pre').length,15);
      assert(!text(doc).includes('Loading the docs'));
      assert.equal(attr(one(doc,n=>attr(n,'download')!==undefined),'href'),route('docs',lang)+'keyala.md');
      assert.equal(fs.readFileSync(path.join(out,route('docs',lang),'keyala.md'),'utf8'),fs.readFileSync(path.join(ROOT,`site/docs/keyala${lang==='en'?'':'.'+lang}.md`),'utf8'));
    }
  }
});

test('TestFlight pages show all seven localized task goals before optional steps', () => {
  const headings = ['Install and sign in', '安裝與登入', 'インストールとサインイン'];
  const forms = {
    'en': 'https://docs.google.com/forms/d/e/1FAIpQLSdqT46s_9CAEkFoq_1YImjQ2rrBOMtjLoo-Ora2qWRq18n_NQ/viewform',
    'zh-Hant': 'https://docs.google.com/forms/d/e/1FAIpQLSeym6NgP_VFPEh_fdAvUWnTndogjh-Jy6ngaDjWQkb574BRlA/viewform',
    'ja': 'https://docs.google.com/forms/d/e/1FAIpQLScOWcYcknRf1R76zxO-cL6VLR_2RQpekE-PN176NG2D8kYlmg/viewform',
  };
  for (const [index, lang] of LANGS.entries()) {
    const doc = read(route('testflight', lang));
    const article = one(doc, n => n.tagName === 'article');
    assert.equal(attr(article, 'lang'), lang);
    assert.equal(text(all(article, n => n.tagName === 'h2')[0]), headings[index]);
    const tasks = all(article, n => attr(n, 'data-task') !== undefined);
    assert.deepEqual(tasks.map(n => attr(n, 'data-task')), ['understand', 'key', 'connect', 'approve', 'deny', 'history', 'revoke']);
    for (const task of tasks) {
      const goal = one(task, n => n.tagName === 'h3');
      const details = one(task, n => n.tagName === 'details');
      assert(task.childNodes.indexOf(goal) < task.childNodes.indexOf(details));
      assert.equal(attr(details, 'open'), undefined);
      assert(text(one(details, n => n.tagName === 'summary')).trim());
      assert(text(details).trim().length > 50);
    }
    const firstTaskInstructions = one(tasks[0], n => n.parentNode === tasks[0] && n.tagName === 'p');
    assert(text(firstTaskInstructions).trim().length > 20);
    one(tasks[2], n => attr(n, 'href') === 'https://iamjoseph331.github.io/interview_prepper/');
    assert.equal(text(one(tasks[3], n => n.tagName === 'code')), 'claude-haiku-5-5');
    assert.equal(attr(one(article, n => attr(n, 'data-video-guide') !== undefined), 'href'), route('guide', lang));
    assert.equal(attr(one(article, n => attr(n, 'href') === 'mailto:support@keyala.io'), 'href'), 'mailto:support@keyala.io');
    const feedback = one(article, n => attr(n, 'id') === 'feedback-form');
    assert.equal(attr(one(feedback, n => n.tagName === 'a'), 'href'), forms[lang]);
    one(article, n => attr(n, 'href') === 'https://apps.apple.com/app/testflight/id899247664');
    assert.equal(all(article, n => n.tagName === 'script').length, 0);
  }
});

test('every TestFlight locale links to both optional examples with their own model instructions', () => {
  const titles = ['Optional example apps', '選做的範例 App', '任意で試せるサンプルアプリ'];
  for (const [index, lang] of LANGS.entries()) {
    const doc = read(route('testflight', lang));
    const examples = one(doc, n => attr(n, 'id') === 'optional-examples');
    assert.equal(text(one(examples, n => n.tagName === 'h2')), titles[index]);
    assert.deepEqual(
      all(examples, n => n.tagName === 'a').map(n => new URL(attr(n, 'href'), SITE).href),
      ['https://keyala.io/examples/flashcards/', 'https://keyala.io/examples/daily-reflection/'],
    );
    assert.deepEqual(all(examples, n => n.tagName === 'code').map(text), ['gpt-4o-mini', 'anthropic/claude-haiku-5.5']);
  }
});

test('translated documents preserve every code sample, inline code token and section structure', () => {
  const en=fs.readFileSync(path.join(ROOT,'site/docs/keyala.md'),'utf8');
  const headings=read(route('docs','en'));
  const ids=all(headings,n=>['h2','h3'].includes(n.tagName)).map(n=>attr(n,'id'));
  const inline=s=>new Set([...s.replace(/```[\s\S]*?```/g,'').matchAll(/`([^`]+)`/g)].map(m=>m[1]));
  for(const lang of ['zh-Hant','ja']){
    const md=fs.readFileSync(path.join(ROOT,`site/docs/keyala.${lang}.md`),'utf8');
    assert.deepEqual(codeBlocks(md),codeBlocks(en));
    assert.deepEqual([...en.matchAll(/^#{1,3} /gm)].map(m=>m[0]),[...md.matchAll(/^#{1,3} /gm)].map(m=>m[0]));
    for(const token of inline(en)) assert(inline(md).has(token),`${lang} missing inline token: ${token}`);
    assert.deepEqual(all(read(route('docs',lang)),n=>['h2','h3'].includes(n.tagName)).map(n=>attr(n,'id')),ids);
  }
});

test('sitemap XML namespaces, locale relationships and indexability', () => {
  execFileSync('python3',['-c',`
import xml.etree.ElementTree as E
from pathlib import Path
root=E.parse(${JSON.stringify(path.join(out,'sitemap.xml'))}).getroot()
ns={'s':'http://www.sitemaps.org/schemas/sitemap/0.9','x':'http://www.w3.org/1999/xhtml'}
assert root.tag=='{'+ns['s']+'}urlset'
urls=root.findall('s:url',ns)
locs=[u.find('s:loc',ns).text for u in urls]
assert len(locs)==len(set(locs))
posts=list(Path(${JSON.stringify(path.join(ROOT,'site/news/posts'))}).glob("*.md"))
assert len(locs)==15+len(posts), locs
for u in urls:
 loc=u.find('s:loc',ns).text
 assert not any('/'+p+'/' in loc for p in ['privacy','support','testflight','review','tutorials','developers'])
 links=u.findall('x:link',ns)
 assert len(links)>=2
 assert any(l.get('href')==loc for l in links)
 assert any(l.get('hreflang')=='x-default' for l in links)
 for l in links: assert l.get('href') in locs
`]);
});

test('generated locale pages and news articles have valid local resources and anchors', () => {
  const pages=[];
  function walk(dir){for(const f of fs.readdirSync(dir,{withFileTypes:true})){if(f.isDirectory())walk(path.join(dir,f.name));else if(f.name==='index.html')pages.push(path.join(dir,f.name));}}
  walk(out);
  const failures=[];
  for(const file of pages){
    const relative=path.relative(out,file);
    if(relative.startsWith('developers/') || relative.startsWith('review/') || relative.startsWith('tutorials/')) continue;
    const url='/'+relative.replace(/index.html$/,''); const doc=parse(fs.readFileSync(file,'utf8'));
    for(const n of all(doc,n=>n.attrs))for(const k of ['href','src','poster']){
      const v=attr(n,k);if(!v || /^(mailto:|data:|tel:)/.test(v))continue;
      const target=new URL(v,SITE+url);if(target.origin!==SITE)continue;
      let dest=path.join(out,decodeURIComponent(target.pathname));if(target.pathname.endsWith('/'))dest=path.join(dest,'index.html');
      if(!fs.existsSync(dest)){failures.push(`${url} -> ${v}`);continue;}
      if(target.hash && dest.endsWith('.html')){
        const anchor=decodeURIComponent(target.hash.slice(1));const d=parse(fs.readFileSync(dest,'utf8'));
        if(!all(d,n=>attr(n,'id')===anchor).length)failures.push(`${url} -> missing anchor ${v}`);
      }
    }
  }
  assert.deepEqual(failures,[]);
});


test('published covers are 1200x630 PNGs and every news article uses its own locale cover', () => {
  for (const lang of LANGS) {
    const file=path.join(out,`media/social/keyala-cover-a-${lang}-v1.png`);
    const image=fs.readFileSync(file);
    assert.deepEqual([...image.subarray(0,8)],[137,80,78,71,13,10,26,10]);
    assert.equal(image.readUInt32BE(16),1200);
    assert.equal(image.readUInt32BE(20),630);
    assert(image.length < 1000000, 'Keep sharing images below 1 MB');
  }
  for (const post of fs.readdirSync(path.join(ROOT,'site/news/posts'))) {
    const match=post.match(/^(.*)\.(en|zh-Hant|ja)\.md$/);
    if(!match)continue;
    const [,stem,lang]=match;
    const doc=read(`/news/${stem}/${lang}/`);
    assert.equal(meta(doc,'twitter:image'),`${SITE}/media/social/keyala-cover-a-${lang}-v1.png`);
    assert.equal(attr(one(doc,n=>n.tagName==='meta' && attr(n,'property')==='og:image'),'content'),meta(doc,'twitter:image'));
  }
});


test('search favicon assets and site name are available to crawlers', () => {
  const png = fs.readFileSync(path.join(out,'brand/favicon-96x96.png'));
  assert.equal(png.readUInt32BE(16),96); assert.equal(png.readUInt32BE(20),96);
  const apple=fs.readFileSync(path.join(out,'brand/apple-touch-icon.png'));
  assert.equal(apple.readUInt32BE(16),180); assert.equal(apple.readUInt32BE(20),180);
  const ico=fs.readFileSync(path.join(out,'favicon.ico'));
  assert.equal(ico.readUInt16LE(2),1); assert.equal(ico.readUInt16LE(4),1);
  assert.equal(ico[6],48); assert.equal(ico[7],48);
  for(const page of PAGES)for(const lang of LANGS){
    const doc=read(route(page,lang));
    assert.equal(attr(one(doc,n=>n.tagName==='link' && attr(n,'rel')==='icon' && attr(n,'type')==='image/png'),'href'),'/brand/favicon-96x96.png');
    assert.equal(attr(one(doc,n=>n.tagName==='link' && attr(n,'rel')==='apple-touch-icon'),'href'),'/brand/apple-touch-icon.png');
    const data=all(doc,n=>n.tagName==='script' && attr(n,'type')==='application/ld+json');
    if(page==='home' && lang==='en')assert.deepEqual(JSON.parse(text(data[0])),{'@context':'https://schema.org','@type':'WebSite',name:'Keyala',url:SITE+'/'});
    else assert.equal(data.length,0);
  }
  assert(fs.readFileSync(path.join(out,'robots.txt'),'utf8').includes('Allow: /'));
});
