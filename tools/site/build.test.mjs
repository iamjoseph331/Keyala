import test, { after } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { parse } from 'parse5';
import { ROOT, SITE, LANGS, PAGES, route, build, all, text } from './build.mjs';

const out = fs.mkdtempSync(path.join(os.tmpdir(), 'keyala-site-test-'));
fs.cpSync(path.join(ROOT, 'site'), out, {recursive:true});
execFileSync('python3', [path.join(ROOT, 'tools/build_news.py'), out]);
build(out);
after(() => fs.rmSync(out, {recursive:true, force:true}));
const attr = (n, k) => n.attrs?.find(a => a.name === k)?.value;
const read = url => parse(fs.readFileSync(path.join(out, url, 'index.html'), 'utf8'));
const one = (doc, fn) => { const nodes=all(doc,fn); assert.equal(nodes.length,1); return nodes[0]; };
const meta = (doc, name) => attr(one(doc,n => n.tagName === 'meta' && attr(n,'name')===name),'content');
const codeBlocks = s => [...s.matchAll(/```[^\n]*\n[\s\S]*?```/g)].map(m=>m[0]);

test('every locale has static content, matching metadata and reciprocal language URLs', () => {
  for (const page of PAGES) for (const lang of LANGS) {
    const url=route(page,lang), doc=read(url);
    assert.equal(attr(one(doc,n=>n.tagName==='html'),'lang'),lang);
    const title=text(one(doc,n=>n.tagName==='title'));
    assert(title.includes('Keyala'));
    assert(meta(doc,'description').length>20);
    assert.equal(meta(doc,'twitter:title'),title);
    assert.equal(attr(one(doc,n=>n.tagName==='meta' && attr(n,'property')==='og:title'),'content'),title);
    assert.equal(attr(one(doc,n=>n.tagName==='link' && attr(n,'rel')==='canonical'),'href'),SITE+url);
    for(const l of [...LANGS,'x-default']) assert.equal(attr(one(doc,n=>n.tagName==='link' && attr(n,'hreflang')===l),'href'),SITE+route(page,l==='x-default'?'en':l));
    assert.equal(all(doc,n=>attr(n,'data-language') && attr(n,'data-language')!==lang).length,0);
    assert(!text(doc).includes('window.KEYALA_STRINGS'));
    assert(!all(doc,n=>n.tagName==='script').some(n=>attr(n,'src')?.includes('i18n.js') || attr(n,'src')?.includes('cdnjs')));
    if(['privacy','support'].includes(page)) assert.equal(meta(doc,'robots'),'noindex');
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
assert len(locs)==12+len(posts), locs
for u in urls:
 loc=u.find('s:loc',ns).text
 assert not any('/'+p+'/' in loc for p in ['privacy','support','review','tutorials','developers'])
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
