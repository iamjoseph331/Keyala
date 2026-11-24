/* Locale is part of the URL. Language links work without JavaScript. */
(() => {
  const links = [...document.querySelectorAll('[data-language-link]')];
  function sync() { links.forEach(a => { a.hash = location.hash; }); }
  sync();
  addEventListener('hashchange', sync);
})();
