const picker = document.getElementById('language');
const languages = ['en', 'zh-Hant', 'ja'];
const requested = new URLSearchParams(location.search).get('lang');
const preferred = navigator.languages || [navigator.language];
const initial = languages.includes(requested) ? requested : preferred.map(v => v.startsWith('ja') ? 'ja' : /^zh-(TW|HK|MO|Hant)/i.test(v) ? 'zh-Hant' : 'en')[0];
function choose(lang) {
  document.documentElement.lang = lang;
  document.querySelectorAll('[data-language]').forEach(section => { section.hidden = section.dataset.language !== lang; });
  picker.value = lang;
  const heading = document.querySelector('[data-language="' + lang + '"] h1');
  if (heading) document.title = heading.textContent + ' — Keyala';
}
picker.addEventListener('change', () => { choose(picker.value); const url = new URL(location.href); url.searchParams.set('lang', picker.value); history.replaceState(null, '', url); });
choose(initial || 'en');
