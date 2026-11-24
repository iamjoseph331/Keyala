/* All articles are present in HTML; JavaScript only adds category filtering. */
(() => {
  const buttons = [...document.querySelectorAll('.filters button')];
  const rows = [...document.querySelectorAll('.list > li')];
  const empty = document.querySelector('.empty');
  function draw(category) {
    if (!buttons.some(b => b.dataset.cat === category)) category = '';
    rows.forEach(li => { li.hidden = !!category && li.dataset.cat !== category; });
    buttons.forEach(b => b.setAttribute('aria-pressed', String(b.dataset.cat === category)));
    empty.hidden = rows.some(li => !li.hidden);
  }
  buttons.forEach(b => b.addEventListener('click', () => {
    const url = new URL(location.href);
    b.dataset.cat ? url.searchParams.set('c', b.dataset.cat) : url.searchParams.delete('c');
    history.replaceState(null, '', url);
    draw(b.dataset.cat);
  }));
  addEventListener('popstate', () => draw(new URLSearchParams(location.search).get('c') || ''));
  draw(new URLSearchParams(location.search).get('c') || '');
})();
