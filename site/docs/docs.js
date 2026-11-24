/* The complete document and table of contents are rendered at build time. */
(() => {
  const doc = document.getElementById('doc');
  const toc = document.getElementById('toc');
  const copy = document.getElementById('copy-md');
  async function flash(button, value) {
    const original = button.textContent;
    try {
      await navigator.clipboard.writeText(value);
      button.textContent = copy.dataset.copiedLabel;
    } catch { button.textContent = copy.dataset.errorLabel; }
    setTimeout(() => { button.textContent = original; }, 2000);
  }
  copy.addEventListener('click', async () => {
    try {
      const r = await fetch('keyala.md');
      if (!r.ok) throw new Error(r.status);
      await flash(copy, await r.text());
    } catch {
      const original = copy.textContent;
      copy.textContent = copy.dataset.errorLabel;
      setTimeout(() => { copy.textContent = original; }, 2000);
    }
  });
  doc.querySelectorAll('pre').forEach(pre => {
    const box = document.createElement('div'); box.className = 'codebox';
    pre.replaceWith(box); box.append(pre);
    const button = document.createElement('button');
    button.type = 'button'; button.textContent = copy.dataset.copyLabel;
    button.addEventListener('click', () => flash(button, pre.innerText));
    box.prepend(button);
  });
  const headings = [...doc.querySelectorAll('h2, h3')];
  const links = [...toc.querySelectorAll('a')];
  function spy() {
    let current = headings[0];
    for (const h of headings) if (h.getBoundingClientRect().top < 120) current = h;
    links.forEach(a => a.classList.toggle('on', current && a.hash === '#' + current.id));
  }
  addEventListener('scroll', spy, { passive: true }); spy();
  const details = document.getElementById('toc-details');
  const mq = matchMedia('(max-width: 800px)');
  const sync = () => { details.open = !mq.matches; };
  sync(); mq.addEventListener('change', sync);
  toc.addEventListener('click', () => { if (mq.matches) details.open = false; });
})();
