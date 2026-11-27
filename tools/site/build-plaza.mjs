// Staging serves the existing examples page from the backend image, without a second page to maintain.
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { parse, parseFragment, serialize } from 'parse5';
const root = new URL('../../', import.meta.url);
const read = path => readFileSync(new URL(path, root), 'utf8');
const page = parse(read('site/examples/index.html'));
const logo = 'data:image/svg+xml;base64,' + Buffer.from(read('site/brand/koala.svg')).toString('base64');
let strings = '';
function visit(node) {
  if (node.childNodes) node.childNodes = node.childNodes.flatMap(child => {
    const attr = name => child.attrs?.find(a => a.name === name);
    if (child.tagName === 'script' && !attr('src')) {
      strings += child.childNodes.map(n => n.value || '').join('');
      return [];
    }
    if (child.tagName === 'script' && attr('src')?.value === '../i18n.js') return [];
    if (child.tagName === 'link' && attr('rel')?.value === 'stylesheet') {
      const css = read('site/base.css').replace(/^@font-face[^\n]*\n/gm, '');
      return parseFragment('<style>' + css + '</style>').childNodes;
    }
    for (const a of child.attrs || []) {
      if (!['href', 'src'].includes(a.name) || a.value.startsWith('#')) continue;
      if (child.tagName === 'script' && a.value === 'plaza.js') a.value = '/examples/plaza.js';
      else if (a.value.endsWith('brand/koala.svg')) a.value = logo;
      else a.value = new URL(a.value, 'https://keyala.io/examples/').href;
      if (a.value === 'https://app.keyala.io/console') a.value = 'https://staging.keyala.io/console';
    }
    visit(child);
    return [child];
  });
}
visit(page);
const head = page.childNodes.find(n => n.tagName === 'html').childNodes.find(n => n.tagName === 'head');
head.childNodes.push(...parseFragment('<meta name="robots" content="noindex, nofollow">').childNodes);
const outputs = {
  'backend/src/plaza.html': serialize(page),
  'backend/src/plaza.js': strings + '\n' + read('site/i18n.js') + '\n' + read('site/examples/plaza.js'),
};
const check = process.argv.includes('--check');
for (const [path, content] of Object.entries(outputs)) {
  if (check) {
    if (read(path) !== content) throw new Error(`${path} is stale; run node tools/site/build-plaza.mjs`);
  } else writeFileSync(fileURLToPath(new URL(path, root)), content);
}
