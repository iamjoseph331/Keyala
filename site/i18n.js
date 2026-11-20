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
