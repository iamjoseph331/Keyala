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
