import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';

for (const app of ['resume-tailor', 'flashcards', 'daily-reflection']) {
  const html = readFileSync(new URL(`../examples/${app}/index.html`, import.meta.url), 'utf8');
  const script = html.match(/<script id="theme-script">([\s\S]*?)<\/script>/)[1];
  test(`${app}: theme follows system until chosen, persists, translates, tolerates blocked storage`, () => {
    for (const blocked of [false, true]) {
      const events = {}, systemEvents = {}, buttonEvents = {}, languageEvents = {}, attrs = {};
      const root = { dataset: {}, lang: 'en' }, saved = {};
      const system = { matches: true, addEventListener: (name, fn) => (systemEvents[name] ||= []).push(fn) };
      const button = { addEventListener: (name, fn) => buttonEvents[name] = fn, setAttribute: (k, v) => attrs[k] = v };
      const context = {
        matchMedia: () => system,
        localStorage: {
          getItem: key => { if (blocked) throw Error('blocked'); return saved[key]; },
          setItem: (key, value) => { if (blocked) throw Error('blocked'); saved[key] = value; },
        },
        document: {
          documentElement: root,
          addEventListener: (name, fn) => events[name] = fn,
          getElementById: id => id === 'theme-toggle' ? button : { addEventListener: (name, fn) => languageEvents[name] = fn },
        },
      };
      vm.runInNewContext(script, context);
      assert.equal(root.dataset.theme, 'dark');
      events.DOMContentLoaded();
      assert.equal(attrs['aria-label'], 'Switch to light mode');
      system.matches = false; systemEvents.change.forEach(fn => fn());
      assert.equal(root.dataset.theme, 'light');
      buttonEvents.click();
      assert.equal(root.dataset.theme, 'dark');
      if (!blocked) assert.equal(saved['keyala-example-theme'], 'dark');
      systemEvents.change.forEach(fn => fn());
      assert.equal(root.dataset.theme, 'dark');
      root.lang = 'zh-Hant'; languageEvents.change();
      assert.equal(attrs['aria-label'], '切換淺色模式');
      root.lang = 'ja'; languageEvents.change();
      assert.equal(attrs['aria-label'], 'ライトモードに切り替える');
      if (!blocked) {
        root.dataset.theme = 'light'; vm.runInNewContext(script, context);
        assert.equal(root.dataset.theme, 'dark');
      }
    }
    assert.match(html, /:root\[data-theme="dark"\]/);
    assert.match(html, /class="theme-sun"/);
    assert.match(html, /class="theme-moon"/);
    assert.equal((html.match(/id="theme-toggle"/g) || []).length, 1);
  });
}
