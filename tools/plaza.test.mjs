import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';
const script = readFileSync(new URL('../site/examples/plaza.js', import.meta.url), 'utf8');
const html = readFileSync(new URL('../site/examples/index.html', import.meta.url), 'utf8');
const tick = () => new Promise(resolve => setImmediate(resolve));
function element() {
  return { children: [], dataset: { open: 'Open app', error: 'Unavailable' }, hidden: true, append(...children) { this.children.push(...children); } };
}
function run(host, response) {
  const list = element(), status = element(), section = element();
  section.querySelector = selector => selector === '.plaza-apps' ? list : status;
  let calls = 0;
  vm.runInNewContext(script, {
    location: { hostname: host }, URL,
    document: { querySelectorAll: () => [section], createElement: () => element() },
    fetch: async path => { calls++; assert.equal(path, '/v1/plaza'); return response; },
  });
  return { section, list, status, calls: () => calls };
}
test('Plaza stays staging-only and has three localized sections', async () => {
  const prod = run('keyala.io'); await tick();
  assert.equal(prod.calls(), 0); assert.equal(prod.section.hidden, true);
  assert.equal((html.match(/class="community-plaza" hidden/g) || []).length, 3);
});
test('Plaza renders names as text and only credential-free HTTPS links', async () => {
  const result = run('staging.keyala.io', { ok: true, json: async () => [
    { name: '<img onerror=alert(1)>', homepage: 'https://example.com/' },
    { name: 'Unsafe', homepage: 'javascript:alert(1)' },
    { name: 'Credentials', homepage: 'https://u:p@example.com/' },
    { name: 'Broken', homepage: 'invalid' }, null,
  ] });
  await tick();
  assert.equal(result.section.hidden, false);
  assert.equal(result.list.children.length, 1);
  const [name, link] = result.list.children[0].children;
  assert.equal(name.textContent, '<img onerror=alert(1)>');
  assert.equal(link.href, 'https://example.com/');
  assert.equal(link.rel, 'noopener noreferrer nofollow');
  assert.equal(result.status.hidden, true);
});
test('Plaza handles empty results, HTTP failures and malformed responses distinctly', async () => {
  const empty = run('staging.keyala.io', { ok: true, json: async () => [] });
  await tick(); assert.equal(empty.status.hidden, false);
  for (const response of [{ ok: false }, { ok: true, json: async () => ({ secret: 'never rendered' }) }]) {
    const failed = run('staging.keyala.io', response);
    await tick(); assert.equal(failed.status.textContent, 'Unavailable');
  }
});
