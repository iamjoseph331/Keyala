window.KEYALA_STRINGS={"en": {"_title": "Example apps - Keyala", "nav.dev": "Developers", "nav.users": "Users", "nav.news": "News", "nav.console": "Console", "foot.privacy": "Privacy", "foot.support": "Support", "foot.contact": "Contact", "skip": "Skip to content"}, "zh-Hant": {"_title": "範例應用 - Keyala", "nav.dev": "開發者", "nav.users": "使用者", "nav.news": "消息", "nav.console": "控制台", "foot.privacy": "隱私", "foot.support": "支援", "foot.contact": "聯絡", "skip": "跳到內容"}, "ja": {"_title": "サンプルアプリ - Keyala", "nav.dev": "開発者", "nav.users": "ユーザー", "nav.news": "ニュース", "nav.console": "コンソール", "foot.privacy": "プライバシー", "foot.support": "サポート", "foot.contact": "お問い合わせ", "skip": "本文へ移動"}};
/* Keyala site language: en | zh-Hant | ja. Two ways to translate a page:
 *   <section data-language="ja">…</section>   whole blocks, only the chosen one is shown
 *   <span data-i18n="key"></span>               short strings from window.KEYALA_STRINGS[lang][key]
 *                                                (data-i18n-html for strings that contain markup)
 * Picker: <select id="language">. Order: ?lang=, remembered choice, browser language, English. */
(() => {
  const LANGS = ['en', 'zh-Hant', 'ja'];
  const fromBrowser = () => (navigator.languages || [navigator.language]).map(v =>
    /^ja/i.test(v) ? 'ja' : /^zh-(TW|HK|MO|Hant)/i.test(v) ? 'zh-Hant' : /^en/i.test(v) ? 'en' : null).find(Boolean) || 'en';
  let saved = null;
  try { saved = localStorage.getItem('keyala-lang'); } catch {}
  const asked = new URLSearchParams(location.search).get('lang');
  const initial = [asked, saved].find(v => LANGS.includes(v)) || fromBrowser();

  function apply(lang) {
    document.documentElement.lang = lang;
    document.querySelectorAll('[data-language]').forEach(el => { el.hidden = el.dataset.language !== lang; });
    const T = (window.KEYALA_STRINGS || {})[lang] || {}, EN = (window.KEYALA_STRINGS || {}).en || {};
    document.querySelectorAll('[data-i18n]').forEach(el => { el.textContent = T[el.dataset.i18n] ?? EN[el.dataset.i18n] ?? el.textContent; });
    document.querySelectorAll('[data-i18n-html]').forEach(el => { el.innerHTML = T[el.dataset.i18nHtml] ?? EN[el.dataset.i18nHtml] ?? el.innerHTML; });
    document.querySelectorAll('[data-i18n-alt]').forEach(el => { el.alt = T[el.dataset.i18nAlt] ?? EN[el.dataset.i18nAlt] ?? el.alt; });
    document.querySelectorAll('[data-i18n-href]').forEach(el => { el.href = T[el.dataset.i18nHref] ?? EN[el.dataset.i18nHref] ?? el.href; });
    const title = T._title || EN._title;
    if (title) document.title = title;
    const picker = document.getElementById('language');
    if (picker) picker.value = lang;
    document.dispatchEvent(new CustomEvent('keyala:lang', { detail: lang }));
  }
  function start() {
    const picker = document.getElementById('language');
    if (picker) picker.addEventListener('change', () => {
      try { localStorage.setItem('keyala-lang', picker.value); } catch {}
      apply(picker.value);
    });
    apply(initial);
    // hidden language blocks shift the page after load, so re-land on #anchor links
    if (location.hash) document.getElementById(decodeURIComponent(location.hash.slice(1)))?.scrollIntoView();
  }
  window.KeyalaLang = { get: () => document.documentElement.lang, apply };
  document.readyState === 'loading' ? document.addEventListener('DOMContentLoaded', start) : start();
})();

// The community preview belongs to staging until Joseph approves a production release.
if (location.hostname === 'staging.keyala.io' || ['localhost', '127.0.0.1'].includes(location.hostname)) {
  const sections = document.querySelectorAll('.community-plaza');
  sections.forEach(section => { section.hidden = false; });
  fetch('/v1/plaza').then(async response => {
    if (!response.ok) throw new Error('plaza_unavailable');
    const apps = await response.json();
    if (!Array.isArray(apps)) throw new Error('invalid_plaza');
    sections.forEach(section => {
      const list = section.querySelector('.plaza-apps');
      apps.forEach(app => {
        if (!app || typeof app.name !== 'string' || !URL.canParse(app.homepage)) return;
        const url = new URL(app.homepage);
        if (url.protocol !== 'https:' || url.username || url.password) return;
        const card = document.createElement('article'); card.className = 'card';
        const name = document.createElement('h3'); name.textContent = app.name;
        const link = document.createElement('a');
        link.className = 'btn primary'; link.href = url.href; link.rel = 'noopener noreferrer nofollow';
        link.textContent = list.dataset.open;
        card.append(name, link); list.append(card);
      });
      section.querySelector('.plaza-status').hidden = list.children.length > 0;
    });
  }).catch(() => sections.forEach(section => {
    const status = section.querySelector('.plaza-status');
    status.textContent = status.dataset.error; status.hidden = false;
  }));
}
