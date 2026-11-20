(async () => {
  const doc = document.getElementById('doc'), toc = document.getElementById('toc');
  let md;
  try {
    const r = await fetch('keyala.md');
    if (!r.ok) throw new Error(r.status);
    md = await r.text();
  } catch { doc.innerHTML = '<p>Could not load the docs. <a href="keyala.md">Read keyala.md directly</a>.</p>'; return; }

  doc.innerHTML = marked.parse(md);
  const heads = [...doc.querySelectorAll('h2, h3')];
  const used = new Set();
  heads.forEach((h) => {
    let id = h.textContent.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'section';
    while (used.has(id)) id += '-2';
    used.add(id); h.id = id;
    h.insertAdjacentHTML('beforeend', ` <a class="anchor" href="#${id}" aria-label="Link to this section">#</a>`);
    toc.insertAdjacentHTML('beforeend', `<a class="${h.tagName.toLowerCase()}" href="#${id}">${h.firstChild.textContent.trim()}</a>`);
  });

  doc.querySelectorAll('pre').forEach((pre) => {
    const box = document.createElement('div'); box.className = 'codebox';
    pre.replaceWith(box); box.append(pre);
    const b = document.createElement('button'); b.type = 'button'; b.textContent = 'Copy';
    b.onclick = () => flash(b, pre.innerText);
    box.prepend(b);
  });

  function flash(btn, text) {
    const old = btn.textContent;
    const done = (t) => { btn.textContent = t; setTimeout(() => (btn.textContent = old), 1500); };
    (navigator.clipboard ? navigator.clipboard.writeText(text) : Promise.reject()).then(() => done(old === 'Copy' ? 'Copied' : 'Copied'), () => done('Press Ctrl+C'));
  }
  document.getElementById('copy-md').onclick = (e) => flash(e.currentTarget, md);

  // scroll-spy
  const links = [...toc.querySelectorAll('a')];
  const spy = () => {
    let cur = heads[0];
    for (const h of heads) if (h.getBoundingClientRect().top < 120) cur = h;
    links.forEach((a) => a.classList.toggle('on', cur && a.hash === '#' + cur.id));
  };
  addEventListener('scroll', spy, { passive: true }); spy();

  // mobile: collapse the TOC
  const det = document.getElementById('toc-details');
  const mq = matchMedia('(max-width: 800px)');
  const sync = () => (det.open = !mq.matches);
  sync(); mq.addEventListener('change', sync);
  toc.addEventListener('click', () => { if (mq.matches) det.open = false; });

  if (location.hash) document.getElementById(decodeURIComponent(location.hash.slice(1)))?.scrollIntoView();
})();
