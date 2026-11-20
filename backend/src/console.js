// Keyala web console. Every server string goes through textContent (h() below), never innerHTML.
// All user-facing text lives in T so translations can be added next to `en`.
const T = { en: {
  title: 'Keyala console', tabScan: 'Scan with your phone', tabPw: 'Account + password',
  scanHelp: 'Open Keyala on your phone, scan this code and approve the sign-in.',
  scanExpired: 'That code expired. Getting a new one…', scanDenied: 'Sign-in was denied on the phone.',
  email: 'Email', password: 'Password', signIn: 'Sign in', pwHelp: 'Set a web password in the Keyala app (Settings) first.',
  signOut: 'Sign out', tabAccount: 'My account', tabApps: 'My apps',
  phoneNote: 'Approving calls and adding or changing keys happen in the Keyala app on your phone, not here.',
  keys: 'Keys', noKeys: 'No keys yet.', added: 'added', apps: 'Connected apps', noGrants: 'No apps connected.',
  lastUsed: 'last used', never: 'never', revoke: 'Revoke', revokeAsk: 'Revoke this app? It will stop working immediately.',
  history: 'Recent history', noHistory: 'No requests yet.', loadOlder: 'Load older', purged: 'Content deleted after 30 days',
  payload: 'Request', response: 'Response', cost: 'cost', unmetered: 'completed, no usage reported', estimate: 'estimate',
  myApps: 'Your apps', noApps: 'No apps yet.', calls7: 'calls (7 d)', create: 'Create app', name: 'Name',
  homepage: 'Homepage (optional)', origins: 'Allowed origins, comma separated (optional, for browser apps)',
  addExisting: 'Add an existing app', secret: 'App secret', add: 'Add',
  secretOnce: 'Copy this secret now. It is shown only once.', copy: 'Copy', copied: 'Copied',
  report: 'Report', days: 'days', rotate: 'Rotate secret', rotateAsk: 'Rotate the secret? The old one stops working at once.',
  requests: 'Requests', completed: 'Completed', denied: 'Denied', failed: 'Failed', users: 'Users', newUsers: 'new users',
  daily: 'Daily requests', models: 'Models', countries: 'Countries', unknown: 'Unknown', when: 'When people use it',
  whenNote: 'Shown in your time zone', busiest: 'Busiest hour', weekend: 'Weekend share',
  expired: 'Your session expired. Sign in again.', back: 'Back', error: 'Something went wrong',
  days_: ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'],
}};
const L = T[(navigator.language || 'en').slice(0, 2)] || T.en;
const root = document.getElementById('app');
let token = null; try { token = sessionStorage.getItem('kc') } catch (e) {}
const setToken = t => { token = t; try { t ? sessionStorage.setItem('kc', t) : sessionStorage.removeItem('kc') } catch (e) {} };

function h(tag, a, ...kids) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(a || {})) {
    if (k.startsWith('on')) e.addEventListener(k.slice(2), v);
    else if (k === 'class') e.className = v;
    else if (v != null && v !== false) e.setAttribute(k, v === true ? '' : v);
  }
  for (const k of kids.flat()) if (k != null && k !== false) e.append(k.nodeType ? k : document.createTextNode(String(k)));
  return e;
}
// every screen change bumps gen; an await that finishes after a newer screen (e.g. sign-out) must not redraw
let gen = 0;
function show(...kids) { gen++; root.replaceChildren(...kids.filter(Boolean)); }

async function api(method, path, body, auth) {
  const hd = { 'content-type': 'application/json' };
  const t = auth === undefined ? token : auth;
  if (t) hd.authorization = 'Bearer ' + t;
  const r = await fetch(path, { method, headers: hd, body: body === undefined ? undefined : JSON.stringify(body) });
  let j = null; try { j = await r.json() } catch (e) {}
  // only the session that made this call may be cleared; a late answer for an old session must not sign out a newer one
  if (r.status === 401 && auth === undefined && t && t === token) { setToken(null); start(L.expired); throw new Error('expired'); }
  if (r.status === 401 && auth === undefined && t !== token) throw new Error('expired');
  if (!r.ok) throw new Error((j && j.error && j.error.message) || L.error);
  return j;
}
const money = n => '$' + (n || 0).toFixed(!n ? 2 : n < 0.01 ? 6 : n < 1 ? 4 : 2);
const when = s => s ? new Date(s).toLocaleString() : L.never;
function tabs(items, sel, on) {
  return h('div', { class: 'tabs', role: 'tablist' }, items.map(([k, label]) =>
    h('button', { role: 'tab', 'aria-selected': String(k === sel), onclick: () => on(k) }, label)));
}
function fail(box) { return e => { if (e.message !== 'expired') box.textContent = e.message; }; }

// ---------- signed out ----------
let poll = null;
function stopPoll() { if (poll) { clearTimeout(poll.t); poll.dead = true; poll = null; } }

async function start(notice) {
  stopPoll();
  const g = ++gen;
  if (token) { try { await api('GET', '/v1/me'); if (g !== gen) return; return account(); } catch (e) { if (g !== gen) return; setToken(null); } }
  signedOut('scan', notice);
}

function signedOut(tab, notice) {
  stopPoll();
  const body = h('div', { class: 'card' });
  show(h('header', {}, h('h1', {}, L.title)), notice ? h('p', { class: 'err' }, notice) : null,
    tabs([['scan', L.tabScan], ['pw', L.tabPw]], tab, k => signedOut(k)), body);
  tab === 'scan' ? scanTab(body) : pwTab(body);
}

function pwTab(body) {
  const err = h('div', { class: 'err' });
  const em = h('input', { type: 'email', autocomplete: 'username', required: true });
  const pw = h('input', { type: 'password', autocomplete: 'current-password', required: true });
  const btn = h('button', { class: 'primary', type: 'submit' }, L.signIn);
  const f = h('form', {}, h('label', {}, L.email, em), h('label', {}, L.password, pw), h('p', {}, btn), err,
    h('p', { class: 'muted' }, L.pwHelp));
  f.addEventListener('submit', async ev => {
    ev.preventDefault(); btn.disabled = true; err.textContent = '';
    try {
      const r = await api('POST', '/v1/auth/password', { username: em.value, password: pw.value }, null);
      if (!f.isConnected) return;
      setToken(r.session_token); start();
    } catch (e) { err.textContent = e.message; btn.disabled = false; }
  });
  body.replaceChildren(f);
}

async function scanTab(body) {
  const status = h('p', { class: 'muted' }, L.scanHelp);
  body.replaceChildren(status);
  const me = poll = { dead: false, t: 0 };
  try {
    const s = await api('POST', '/v1/web/login', undefined, null);
    if (me.dead) return;
    body.replaceChildren(h('img', { class: 'qr', src: s.qr_url, alt: 'QR' }), status,
      h('p', { class: 'muted' }, s.url));
    const tick = async () => {
      if (me.dead) return;
      try {
        const r = await api('GET', '/v1/web/login/' + encodeURIComponent(s.login_id), undefined, s.login_secret);
        if (me.dead) return;
        if (r.session_token) { setToken(r.session_token); return start(); }
        if (r.status === 'expired') return scanTab(body);
        if (r.status === 'denied') { status.textContent = L.scanDenied; me.t = setTimeout(() => scanTab(body), 2500); return; }
      } catch (e) { if (me.dead) return; status.textContent = e.message; }
      me.t = setTimeout(tick, 2000);
    };
    me.t = setTimeout(tick, 2000);
  } catch (e) { status.textContent = e.message; }
}

// ---------- signed in ----------
let view = 'account';
async function account() {
  let who; const g = ++gen;
  try { who = await api('GET', '/v1/me'); } catch (e) { return; }
  if (g !== gen || !token) return;
  const body = h('div');
  show(h('header', {}, h('h1', {}, L.title), h('span', { class: 'muted' }, who.email),
    h('button', { onclick: async () => { try { await api('POST', '/v1/me/logout'); } catch (e) {} setToken(null); start(); } }, L.signOut)),
    tabs([['account', L.tabAccount], ['apps', L.tabApps]], view, k => { view = k; account(); }), body);
  view === 'account' ? accountView(body) : appsView(body);
}

async function accountView(body) {
  const err = h('div', { class: 'err' });
  body.append(h('p', { class: 'muted' }, L.phoneNote), err);
  const keysBox = h('div', { class: 'card' }), grantsBox = h('div', { class: 'card' }), histBox = h('div', { class: 'card' });
  body.append(h('h2', {}, L.keys), keysBox, h('h2', {}, L.apps), grantsBox, h('h2', {}, L.history), histBox);
  try {
    const keys = await api('GET', '/v1/me/keys');
    keysBox.replaceChildren(...(keys.length ? keys.map(k => h('div', { class: 'row' },
      h('div', {}, h('b', {}, k.vendor), ' …', k.last4), h('div', { class: 'muted' }, L.added + ' ' + when(k.created_at)))) : [h('span', { class: 'muted' }, L.noKeys)]));
    const loadGrants = async () => {
      const gs = await api('GET', '/v1/me/grants');
      grantsBox.replaceChildren(...(gs.length ? gs.map(g => h('div', { class: 'row' },
        h('div', {}, h('b', {}, g.app.name), ' ', h('span', { class: 'muted' }, g.vendor + ', ' + g.policy.mode),
          h('div', { class: 'muted' }, (g.app.homepage || '') + ' ' + L.lastUsed + ' ' + when(g.last_used_at))),
        h('button', { class: 'danger', onclick: async () => {
          if (!confirm(L.revokeAsk)) return;
          try { await api('DELETE', '/v1/me/grants/' + encodeURIComponent(g.id)); loadGrants(); } catch (e) { fail(err)(e); }
        } }, L.revoke))) : [h('span', { class: 'muted' }, L.noGrants)]));
    };
    await loadGrants();
    let last = null; const list = h('div'); const more = h('button', {}, L.loadOlder);
    const page = async () => {
      const rs = await api('GET', '/v1/me/requests?limit=25' + (last ? '&before_id=' + encodeURIComponent(last) : ''));
      if (!last && !rs.length) { histBox.replaceChildren(h('span', { class: 'muted' }, L.noHistory)); return; }
      for (const r of rs) list.append(requestRow(r, err));
      if (rs.length) last = rs[rs.length - 1].id;
      more.hidden = rs.length < 25;
    };
    more.addEventListener('click', () => page().catch(fail(err)));
    histBox.replaceChildren(list, more);
    await page();
  } catch (e) { fail(err)(e); }
}

function requestRow(r, err) {
  const detail = h('div', { hidden: true });
  const head = h('button', { type: 'button', class: 'row reqhead', 'aria-expanded': 'false' },
    h('span', {}, h('b', {}, r.app.name), ' ', h('span', { class: 'muted' }, r.vendor + ', ' + r.model + ', ' + r.status)),
    h('span', { class: 'muted' }, when(r.created_at) + ', ' + L.cost + ' ' + (r.actual_usd != null ? money(r.actual_usd) : money(r.estimated_usd) + ' (' + L.estimate + ')')));
  let loaded = false;
  head.addEventListener('click', async () => {
    detail.hidden = !detail.hidden;
    head.setAttribute('aria-expanded', String(!detail.hidden));
    if (detail.hidden || loaded) return;
    try {
      const d = await api('GET', '/v1/me/requests/' + encodeURIComponent(r.id)); loaded = true;
      if (d.purged) detail.replaceChildren(h('p', { class: 'muted' }, L.purged));
      else detail.replaceChildren(h('h3', {}, L.payload), h('pre', {}, JSON.stringify(d.payload, null, 2)),
        h('h3', {}, L.response), h('pre', {}, JSON.stringify(d.response, null, 2)));
    } catch (e) { fail(err)(e); }
  });
  return h('div', {}, head, detail);
}

async function appsView(body) {
  const err = h('div', { class: 'err' }), list = h('div', { class: 'card' }), notice = h('div'), rep = h('div');
  const name = h('input', { maxlength: 100 }), home = h('input', { type: 'url' }), orig = h('input', { placeholder: 'https://example.com' });
  const sec = h('input', { type: 'password', autocomplete: 'off' });
  const secretBox = (s, text) => h('div', { class: 'card' }, h('b', {}, text), h('div', { class: 'secret' }, s),
    h('button', { onclick: ev => { navigator.clipboard.writeText(s).then(() => { ev.target.textContent = L.copied; }); } }, L.copy));
  const create = h('form', { class: 'card' }, h('h3', {}, L.create), h('label', {}, L.name, name), h('label', {}, L.homepage, home),
    h('label', {}, L.origins, orig), h('p', {}, h('button', { class: 'primary', type: 'submit' }, L.create)));
  create.addEventListener('submit', async ev => {
    ev.preventDefault(); err.textContent = '';
    const o = orig.value.split(',').map(x => x.trim()).filter(Boolean);
    try {
      const r = await api('POST', '/v1/me/apps', { name: name.value, homepage: home.value || null, allowed_origins: o.length ? o : null });
      notice.replaceChildren(secretBox(r.app_secret, L.secretOnce + ' (' + r.app_id + ')')); name.value = ''; home.value = ''; orig.value = ''; load();
    } catch (e) { fail(err)(e); }
  });
  const claim = h('form', { class: 'card' }, h('h3', {}, L.addExisting), h('label', {}, L.secret, sec),
    h('p', {}, h('button', { type: 'submit' }, L.add)));
  claim.addEventListener('submit', async ev => {
    ev.preventDefault(); err.textContent = '';
    try { await api('POST', '/v1/me/apps/claim', { app_secret: sec.value }); sec.value = ''; load(); } catch (e) { fail(err)(e); }
  });
  body.append(err, notice, h('h2', {}, L.myApps), list, create, claim, rep);
  async function load() {
    try {
      const apps = await api('GET', '/v1/me/apps');
      list.replaceChildren(...(apps.length ? apps.map(a => h('div', { class: 'row' },
        h('div', {}, h('b', {}, a.name), h('div', { class: 'muted' }, a.id + ', ' + a.requests_7d + ' ' + L.calls7 + ', ' + money(a.cost_usd_7d))),
        h('button', { onclick: () => report(a, 30) }, L.report))) : [h('span', { class: 'muted' }, L.noApps)]));
    } catch (e) { fail(err)(e); }
  }
  async function report(a, days) {
    err.textContent = '';
    try {
      const r = await api('GET', '/v1/me/apps/' + encodeURIComponent(a.id) + '/report?days=' + days);
      rep.replaceChildren(reportView(a, r, days, d => report(a, d), notice, secretBox, err));
      rep.scrollIntoView({ behavior: 'smooth' });
    } catch (e) { fail(err)(e); }
  }
  load();
}

function reportView(a, r, days, again, notice, secretBox, err) {
  const t = r.totals, stat = (l, v) => h('div', { class: 'stat' }, h('b', {}, v), l[0].toUpperCase() + l.slice(1));
  const max = Math.max(1, ...r.daily.map(d => d.requests));
  const bars = h('div', { class: 'bars' }, r.daily.map(d => h('i', { style: 'height:' + (d.requests / max * 100) + '%', title: d.date + ': ' + d.requests })));
  const rot = h('button', { class: 'danger', onclick: async () => {
    if (!confirm(L.rotateAsk)) return;
    try { const s = await api('POST', '/v1/me/apps/' + encodeURIComponent(a.id) + '/secret'); notice.replaceChildren(secretBox(s.app_secret, L.secretOnce)); notice.scrollIntoView(); } catch (e) { fail(err)(e); }
  } }, L.rotate);
  const sel = h('div', { class: 'tabs' }, [7, 30, 90].map(d => h('button', { 'aria-selected': String(d === days), onclick: () => again(d) }, d + ' ' + L.days)));
  return h('div', {}, h('h2', {}, a.name + ', ' + L.report), sel,
    h('div', { class: 'grid' }, stat(L.requests, t.requests), stat(L.completed, t.completed), stat(L.denied, t.denied), stat(L.failed, t.failed),
      stat(L.cost, money(t.cost_usd)), stat(L.unmetered, t.unmetered), stat(L.users, t.users + ' (' + t.new_users + ' ' + L.newUsers + ')')),
    h('h3', {}, L.daily), bars,
    h('h3', {}, L.models), h('div', { class: 'card' }, r.models.map(m => h('div', { class: 'row' }, h('div', {}, m.vendor + ', ' + m.model), h('div', { class: 'muted' }, m.requests + ', ' + money(m.cost_usd))))),
    h('h3', {}, L.countries), h('div', { class: 'card' }, r.countries.map(c => h('div', { class: 'row' }, h('div', {}, c.country || L.unknown), h('div', { class: 'muted' }, c.requests + ', ' + money(c.cost_usd))))),
    heatmap(r.hours_utc), h('p', {}, rot));
}

// hours_utc[weekday][hour] is UTC; shift by the browser's offset so the viewer sees their own clock
function heatmap(utc) {
  const off = -new Date().getTimezoneOffset() / 60; // hours east of UTC (whole hours; half-hour zones round)
  const shift = Math.round(off), g = Array.from({ length: 7 }, () => Array(24).fill(0));
  for (let d = 0; d < 7; d++) for (let hr = 0; hr < 24; hr++) {
    const t = d * 24 + hr + shift, i = ((t % 168) + 168) % 168;
    g[Math.floor(i / 24)][i % 24] += utc[d][hr];
  }
  const max = Math.max(1, ...g.flat());
  const grid = h('div', { class: 'heat' }, h('em'), Array.from({ length: 24 }, (_, i) => h('em', {}, i % 6 === 0 ? i : '')));
  g.forEach((row, d) => { grid.append(h('em', {}, L.days_[d]), ...row.map((n, hr) => h('span', { style: '--a:' + (n / max).toFixed(2), title: L.days_[d] + ' ' + hr + ':00, ' + n }))); });
  const byHour = Array(24).fill(0); g.forEach(r => r.forEach((n, i) => byHour[i] += n));
  const total = byHour.reduce((a, b) => a + b, 0), best = byHour.indexOf(Math.max(...byHour));
  const wk = total ? Math.round((g[0].concat(g[6]).reduce((a, b) => a + b, 0)) / total * 100) : 0;
  return h('div', {}, h('h3', {}, L.when), grid, h('p', { class: 'muted' },
    L.whenNote + ' (' + Intl.DateTimeFormat().resolvedOptions().timeZone + ')' + (total ? ', ' + L.busiest + ' ' + best + ':00, ' + L.weekend + ' ' + wk + '%' : '')));
}

start();
