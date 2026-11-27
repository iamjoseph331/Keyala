// Keyala web console. Every server string goes through textContent (h() below), never innerHTML.
// All user-facing text lives in T so translations can be added next to `en`.
const T = { en: {
  title: 'Keyala console', tabScan: 'Scan with your phone', tabPw: 'Account + password',
  scanHelp: 'Open Keyala on your phone, scan this code and approve the sign-in.',
  scanExpired: 'That code expired. Getting a new one…', scanDenied: 'Sign-in was denied on the phone.',
  email: 'Email', password: 'Password', signIn: 'Sign in', pwHelp: 'Set a web password in the Keyala app (Settings) first.',
  signOut: 'Sign out', tabAccount: 'My account', tabApps: 'My apps',
  phoneNote: 'Approving calls and adding or changing keys happen in the Keyala app on your iPhone. Here you can look and revoke.',
  accountTitle: 'Your keys and receipts', appsTitle: 'Apps you build', keysNote: 'Only the last four characters are ever shown. Keys are encrypted at rest.',
  historyNote: 'Prompts and responses are deleted after 30 days. Costs stay.',
  modes: { ask: 'Ask each time', count: 'Next N calls', budget: 'Up to a budget', trust: 'Trust' },
  keys: 'Keys', noKeys: 'No keys yet.', added: 'added', apps: 'Connected apps', noGrants: 'No apps connected.',
  lastUsed: 'last used', never: 'never', revoke: 'Revoke', revokeAsk: 'Revoke this app? It will stop working immediately.',
  history: 'Receipts', noHistory: 'No requests yet.', loadOlder: 'Load older', purged: 'Content deleted after 30 days',
  payload: 'Request', response: 'Response', cost: 'cost', unmetered: 'completed, no usage reported', estimate: 'estimate',
  myApps: 'Your apps', noApps: 'No apps yet. Let\u2019s make your first one.', noAppsHelp: 'Create an app to get an App ID and an App secret. Then add the connect button and change your base URL.', steps: 'See the steps', calls7: 'calls (7 d)', create: 'Create app', createTitle: 'Create an app', name: 'App name', nameHint: 'Users see this on their phone when you ask for a call.',
  homepage: 'Homepage (optional)', origins: 'Allowed origins (for apps that run in a browser)', originsHint: 'Scheme and host only, no path. Comma separated, up to 10; http:// only for localhost. Leave empty if you call Keyala from a server.',
  addExisting: 'Already have an app?', claimHint: 'Made one earlier with POST /v1/apps? Paste its app secret to add it to your account.', secret: 'App secret', add: 'Add app',
  tipsTitle: 'Good to know', tips: ['The secret is shown once. Store it like a password.', 'You can rotate it any time; the old one stops at once.', 'Your report never includes users\u2019 prompts or emails.'],
  secretOnce: 'Copy this secret now. It is shown only once.', copy: 'Copy', copied: 'Copied',
  report: 'Report', days: 'days', edit: 'Edit', save: 'Save', cancel: 'Cancel', saved: 'Saved.', del: 'Delete',
  editTitle: 'Edit app', originsNote: 'New connections use the new values. People already connected keep working.',
  delAsk: 'Delete this app? Its secret stops working, every connected user loses access at once, and waiting calls are refused. This cannot be undone. Type the app name to confirm:',
  delMismatch: 'The name did not match, nothing was deleted.', deleted: 'App deleted.', origins_: 'origins', serverOnly: 'server app (no browser origins)', rotate: 'Rotate secret', rotateAsk: 'Rotate the secret? The old one stops working at once.',
  requests: 'Requests', completed: 'Completed', denied: 'Denied', failed: 'Failed', users: 'Users', newUsers: 'new users',
  daily: 'Daily requests', models: 'Models', countries: 'Countries', unknown: 'Unknown', when: 'When people use it',
  whenNote: 'Shown in your time zone', busiest: 'Busiest hour', weekend: 'Weekend share',
  expired: 'Your session expired. Sign in again.', back: 'Back', error: 'Something went wrong',
  days_: ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'],
  plazaList: 'List in Plaza', plazaWithdraw: 'Withdraw from Plaza', plazaNeedHome: 'Add an https:// homepage to list this app in Plaza.',
  plazaStatus: { pending: 'Plaza: waiting for review', approved: 'Plaza: listed', rejected: 'Plaza: not approved' },
  plazaAsk: 'Submit this app for Plaza? If approved, only its name and homepage become public. Editing either one removes it from Plaza.',
  plazaId: 'Your Plaza reviewer ID (add to KEYALA_PLAZA_REVIEWERS)', plazaTitle: 'Plaza review', plazaNone: 'Nothing here.', approve: 'Approve', reject: 'Reject', takeDown: 'Take down', open: 'open',
  plazaTabs: { pending: 'Pending', approved: 'Listed', rejected: 'Rejected' },
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
function clearCouponKey(panel = root) {
  const key = panel.querySelector('[data-coupon-key]');
  if (key) key.value = '';
}
function show(...kids) { gen++; clearCouponKey(); root.replaceChildren(...kids.filter(Boolean)); }

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
const C = {
  en: { btn: 'Coupons', intro: 'Share the link or code so users can claim a fixed amount. They review the full prompt and approve each call on their phone. Use a separate API key for each batch and set a spending limit with your provider.',
    vendor: 'Provider', models: 'Models (comma separated)', key: 'Dedicated API key (stored encrypted, never shown again)', per: 'Per coupon (USD)', total: 'Total budget (USD)', days: 'Valid for (days after claiming)', deadline: 'Claim until', create: 'Create batch',
    none: 'No batches yet.', avail: 'Available to claim', link: 'Claim page', qr: 'Claim QR code', revoke: 'Revoke', revokeAsk: 'Revoke this batch? Unused balances return, waiting calls are denied and the key is wiped.', revoked: 'Revoked', stopped: 'Stopped', report: 'Report',
    claimed: 'Coupons claimed', spent: 'Total spent', held: 'Total held', returned: 'Returned to batch', overshoot: 'Overshoot',
    actual: 'Provider-reported spend', estimated: 'Estimates charged for completed calls', uncertain: 'Held for uncertain calls', firstSuccess: 'Users with a successful call',
    outcomes: 'Request outcomes', modelSpend: 'Spend by model', noModels: 'No settled calls yet.', settled: 'Settled calls',
    status: { pending: 'Awaiting approval', approved: 'Approved', sending: 'Sending', completed: 'Completed', denied: 'Denied', failed: 'Failed', expired: 'Expired', uncertain: 'Uncertain' },
    email: 'Email alerts on', noEmail: 'Email alerts off', close: 'Close', copy: 'Copy link', copied: 'Link copied', copyFailed: 'Could not copy the link.' },
  zh: { btn: '優惠券', intro: '分享連結或代碼，讓使用者領取固定額度。每次呼叫都要在手機上查看完整提示詞並核准。請為每個批次使用專用 API 金鑰，並在供應商端設定支出上限。',
    vendor: '供應商', models: '模型（以逗號分隔）', key: '專用 API 金鑰（加密儲存，之後不再顯示）', per: '每張金額（USD）', total: '總預算（USD）', days: '領取後有效天數', deadline: '領取期限', create: '建立批次',
    none: '尚無批次。', avail: '可供領取', link: '領取頁面', qr: '領取用 QR 碼', revoke: '撤銷', revokeAsk: '撤銷此批次？未用餘額會退回，等待中的呼叫會被拒絕，金鑰會被清除。', revoked: '已撤銷', stopped: '已停止', report: '報告',
    claimed: '已領取的優惠券', spent: '總支出', held: '總保留金額', returned: '已退回批次', overshoot: '超支',
    actual: '供應商回報的支出', estimated: '已完成呼叫的估計扣款', uncertain: '結果未確認的保留金額', firstSuccess: '曾成功完成呼叫的使用者',
    outcomes: '呼叫結果', modelSpend: '各模型支出', noModels: '尚無已結算的呼叫。', settled: '已結算的呼叫',
    status: { pending: '等待核准', approved: '已核准', sending: '傳送中', completed: '已完成', denied: '已拒絕', failed: '失敗', expired: '已過期', uncertain: '結果未確認' },
    email: '已開啟電子郵件通知', noEmail: '電子郵件通知未開啟', close: '關閉', copy: '複製連結', copied: '已複製連結', copyFailed: '無法複製連結。' },
  ja: { btn: 'クーポン', intro: 'リンクやコードを共有すると、ユーザーが一定額を受け取れます。ユーザーはスマートフォンで毎回プロンプト全文を確認し、呼び出しを承認します。バッチごとに専用の API キーを使い、プロバイダ側でも利用上限を設定してください。',
    vendor: 'プロバイダ', models: 'モデル（カンマ区切り）', key: '専用 API キー（暗号化して保存、再表示されません）', per: '1枚あたり（USD）', total: '総予算（USD）', days: '受け取り後の有効日数', deadline: '受け取り期限', create: 'バッチを作成',
    none: 'バッチはまだありません。', avail: '受け取り可能額', link: '受け取りページ', qr: '受け取り用 QR コード', revoke: '取り消す', revokeAsk: 'このバッチを取り消しますか？未使用の残高は戻り、待機中の呼び出しは拒否され、キーは消去されます。', revoked: '取り消し済み', stopped: '停止', report: 'レポート',
    claimed: '受け取り済みのクーポン', spent: '支出総額', held: '確保中の総額', returned: 'バッチへの返却額', overshoot: '超過',
    actual: 'プロバイダ報告に基づく支出', estimated: '完了した呼び出しの見積もりによる請求額', uncertain: '結果が不明な呼び出しの確保額', firstSuccess: '呼び出しに成功したユーザー',
    outcomes: 'リクエストの結果', modelSpend: 'モデル別の支出', noModels: 'まだ精算された呼び出しはありません。', settled: '精算済みの呼び出し',
    status: { pending: '承認待ち', approved: '承認済み', sending: '送信中', completed: '完了', denied: '拒否', failed: '失敗', expired: '期限切れ', uncertain: '結果不明' },
    email: 'メール通知オン', noEmail: 'メール通知オフ', close: '閉じる', copy: 'リンクをコピー', copied: 'リンクをコピーしました', copyFailed: 'リンクをコピーできませんでした。' },
};
const couponLanguage = (navigator.language || 'en').slice(0, 2);
const CL = C[couponLanguage] || C.en;
const couponMoney = new Intl.NumberFormat(navigator.language, { style: 'currency', currency: 'USD', maximumFractionDigits: 6 }).format;
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
    body.replaceChildren(h('img', { class: 'qr', src: s.qr_url, alt: 'QR' }), status);
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
  show(h('header', {}, h('h1', {}, view === 'account' ? L.accountTitle : L.appsTitle), h('span', { class: 'muted' }, who.email),
    h('button', { onclick: async () => { clearCouponKey(); try { await api('POST', '/v1/me/logout'); } catch (e) {} setToken(null); start(); } }, L.signOut)),
    tabs([['account', L.tabAccount], ['apps', L.tabApps]], view, k => { view = k; account(); }), body);
  view === 'account' ? accountView(body) : appsView(body, who);
}

async function accountView(body) {
  const err = h('div', { class: 'err' });
  body.append(h('p', { class: 'note' }, L.phoneNote), err);
  const keysBox = h('div', { class: 'card' }), grantsBox = h('div', { class: 'card' }), histBox = h('div', { class: 'card' });
  body.append(h('h2', {}, L.keys), keysBox, h('p', { class: 'muted' }, L.keysNote), h('h2', {}, L.apps), grantsBox, h('h2', {}, L.history), h('p', { class: 'muted' }, L.historyNote), histBox);
  try {
    const keys = await api('GET', '/v1/me/keys');
    keysBox.replaceChildren(...(keys.length ? keys.map(k => h('div', { class: 'row' },
      h('div', {}, h('b', {}, k.vendor), ' …', k.last4), h('div', { class: 'muted' }, L.added + ' ' + when(k.created_at)))) : [h('span', { class: 'muted' }, L.noKeys)]));
    const loadGrants = async () => {
      const gs = await api('GET', '/v1/me/grants');
      grantsBox.replaceChildren(...(gs.length ? gs.map(g => h('div', { class: 'row' },
        h('div', {}, h('b', {}, g.app.name), h('span', { class: 'chip' }, L.modes[g.policy.mode] || g.policy.mode), ' ', h('span', { class: 'muted' }, g.vendor),
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

// Reviewers only (who.plaza_reviewer): what the server lists is name + homepage, nothing private.
// Each decision sends back the name and homepage that were on screen, so a stale row is refused.
function plazaReview(box) {
  let st = 'pending'; const err = h('div', { class: 'err' }), list = h('div', { class: 'card' }), bar = h('div');
  box.append(h('h2', {}, L.plazaTitle), bar, err, list);
  async function load() {
    bar.replaceChildren(...Object.keys(L.plazaTabs).map(k => h('button', { class: k === st ? 'primary' : '', onclick: () => { st = k; load(); } }, L.plazaTabs[k])), ' ');
    try {
      const rows = await api('GET', '/v1/plaza/review?status=' + st);
      list.replaceChildren(...(rows.length ? rows.map(r => {
        const decide = d => async () => {
          err.textContent = '';
          try { await api('POST', '/v1/plaza/review/' + encodeURIComponent(r.app_id), { decision: d, name: r.name, homepage: r.homepage }); } catch (e) { fail(err)(e); }
          load();
        };
        const btns = st === 'pending' ? [h('button', { class: 'primary', onclick: decide('approve') }, L.approve), ' ', h('button', { class: 'danger', onclick: decide('reject') }, L.reject)]
          : st === 'approved' ? [h('button', { class: 'danger', onclick: decide('withdraw') }, L.takeDown)] : [];
        const hp = r.homepage || '';
        return h('div', { class: 'row' }, h('div', {}, h('b', {}, r.name), h('div', { class: 'muted' }, hp, ' ',
          hp.startsWith('https://') ? h('a', { href: hp, target: '_blank', rel: 'noopener noreferrer nofollow' }, L.open) : null)), h('div', {}, btns));
      }) : [h('span', { class: 'muted' }, L.plazaNone)]));
    } catch (e) { fail(err)(e); }
  }
  load();
}

async function appsView(body, who) {
  const err = h('div', { class: 'err' }), list = h('div', { class: 'card' }), notice = h('div'), rep = h('div');
  const replacePanel = (...kids) => { clearCouponKey(rep); rep.replaceChildren(...kids); };
  const name = h('input', { maxlength: 100, placeholder: 'Interview Prepper' }), home = h('input', { type: 'url', placeholder: 'https://your-app.example' }), orig = h('input', { placeholder: 'https://you.github.io, http://localhost:5173' });
  const sec = h('input', { type: 'password', autocomplete: 'off', placeholder: 'keyala_app_\u2026' });
  const secretBox = (s, text) => h('div', { class: 'card' }, h('b', {}, text), h('div', { class: 'secret' }, s),
    h('button', { onclick: ev => { navigator.clipboard.writeText(s).then(() => { ev.target.textContent = L.copied; }); } }, L.copy));
  const create = h('form', { class: 'card' }, h('h2', {}, L.createTitle), h('label', {}, L.name, name, h('span', { class: 'hint' }, L.nameHint)), h('label', {}, L.homepage, home),
    h('label', {}, L.origins, orig, h('span', { class: 'hint' }, L.originsHint)), h('p', {}, h('button', { class: 'primary', type: 'submit' }, L.create)));
  create.addEventListener('submit', async ev => {
    ev.preventDefault(); err.textContent = '';
    const o = orig.value.split(',').map(x => x.trim()).filter(Boolean);
    try {
      const r = await api('POST', '/v1/me/apps', { name: name.value, homepage: home.value || null, allowed_origins: o.length ? o : null });
      notice.replaceChildren(secretBox(r.app_secret, L.secretOnce + ' (' + r.app_id + ')')); name.value = ''; home.value = ''; orig.value = ''; load();
    } catch (e) { fail(err)(e); }
  });
  const claim = h('form', { class: 'card' }, h('h3', {}, L.addExisting), h('p', { class: 'muted' }, L.claimHint), h('label', {}, L.secret, sec),
    h('p', {}, h('button', { type: 'submit' }, L.add)));
  claim.addEventListener('submit', async ev => {
    ev.preventDefault(); err.textContent = '';
    try { await api('POST', '/v1/me/apps/claim', { app_secret: sec.value }); sec.value = ''; load(); } catch (e) { fail(err)(e); }
  });
  const tips = h('div', { class: 'card tip' }, h('h3', {}, L.tipsTitle), h('ul', {}, L.tips.map(t => h('li', {}, t))));
  body.append(err, notice, h('h2', {}, L.myApps), list, h('div', { class: 'cols' }, create, h('div', { class: 'stack' }, claim, tips)), rep);
  if (who && who.plaza_review_id) body.append(h('p', { class: 'muted' }, L.plazaId + ': ', h('code', {}, who.plaza_review_id), ' ',
    h('button', { onclick: ev => { navigator.clipboard.writeText(who.plaza_review_id).then(() => { ev.target.textContent = L.copied; }); } }, L.copy)));
  if (who && who.plaza_reviewer) plazaReview(body);
  async function plaza(a) {
    err.textContent = '';
    if (!a.plaza || a.plaza === 'rejected') { if (!confirm(L.plazaAsk)) return; }
    try { await api(!a.plaza || a.plaza === 'rejected' ? 'POST' : 'DELETE', '/v1/me/apps/' + encodeURIComponent(a.id) + '/plaza'); load(); } catch (e) { fail(err)(e); }
  }
  async function load() {
    try {
      const apps = await api('GET', '/v1/me/apps');
      list.replaceChildren(...(apps.length ? apps.map(a => h('div', { class: 'row' },
        h('div', {}, h('b', {}, a.name), h('div', { class: 'muted' }, a.id + ', ' + a.requests_7d + ' ' + L.calls7 + ', ' + money(a.cost_usd_7d)),
          h('div', { class: 'muted' }, a.allowed_origins ? L.origins_ + ': ' + a.allowed_origins.join(', ') : L.serverOnly),
          !who || !who.plaza_enabled ? null : h('div', { class: 'muted' }, a.plaza ? L.plazaStatus[a.plaza] + ' ' : '',
            a.homepage || a.plaza ? h('button', { onclick: () => plaza(a) }, a.plaza && a.plaza !== 'rejected' ? L.plazaWithdraw : L.plazaList) : L.plazaNeedHome)),
        h('div', {}, h('button', { onclick: () => report(a, 30) }, L.report), ' ', h('button', { onclick: () => coupons(a) }, CL.btn), ' ', h('button', { onclick: () => edit(a) }, L.edit), ' ',
          h('button', { class: 'danger', onclick: () => del(a) }, L.del)))) : [h('div', { class: 'empty' }, h('div', {}, h('b', {}, L.noApps), h('span', { class: 'muted' }, L.noAppsHelp, ' '), h('a', { href: 'https://keyala.io/docs/' }, L.steps)))]));
    } catch (e) { fail(err)(e); }
  }
  function edit(a) {
    err.textContent = '';
    const n = h('input', { maxlength: 100, value: a.name }), hp = h('input', { type: 'url', value: a.homepage || '' });
    const og = h('input', { placeholder: 'https://example.com', value: (a.allowed_origins || []).join(', ') });
    const f = h('form', { class: 'card' }, h('h3', {}, L.editTitle + ': ' + a.id), h('label', {}, L.name, n), h('label', {}, L.homepage, hp),
      h('label', {}, L.origins, og), h('p', { class: 'muted' }, L.originsNote),
      h('p', {}, h('button', { class: 'primary', type: 'submit' }, L.save), ' ', h('button', { type: 'button', onclick: () => rep.replaceChildren() }, L.cancel)));
    f.addEventListener('submit', async ev => {
      ev.preventDefault(); err.textContent = '';
      const o = og.value.split(',').map(x => x.trim()).filter(Boolean);
      try {
        await api('PUT', '/v1/me/apps/' + encodeURIComponent(a.id), { name: n.value, homepage: hp.value || null, allowed_origins: o.length ? o : null });
        if (!f.isConnected) return;
        rep.replaceChildren(h('p', { class: 'muted' }, L.saved)); load();
      } catch (e) { fail(err)(e); }
    });
    replacePanel(f); f.scrollIntoView({ behavior: 'smooth' }); n.focus();
  }
  async function del(a) {
    err.textContent = '';
    const typed = prompt(L.delAsk + '\n\n' + a.name);
    if (typed === null) return;
    if (typed.trim() !== a.name) { err.textContent = L.delMismatch; return; }
    try {
      await api('DELETE', '/v1/me/apps/' + encodeURIComponent(a.id));
      replacePanel(); notice.replaceChildren(h('p', { class: 'muted' }, L.deleted)); load();
    } catch (e) { fail(err)(e); }
  }
  function coupons(a) {
    const base = '/v1/me/apps/' + encodeURIComponent(a.id) + '/coupons';
    const box = h('section', { class: 'card', lang: couponLanguage === 'zh' ? 'zh-Hant' : couponLanguage === 'ja' ? 'ja' : 'en' });
    const listBox = h('div'), out = h('div', { 'aria-live': 'polite' });
    let reportVersion = 0;
    const error = h('div', { class: 'err', role: 'alert' });
    const vendor = h('select', {}, ['openai', 'anthropic', 'openrouter'].map(v => h('option', { value: v }, v)));
    const models = h('input', { placeholder: 'gpt-4o-mini', required: true });
    const key = h('input', { type: 'password', autocomplete: 'off', 'data-coupon-key': true, required: true });
    const usd = { type: 'number', step: '0.000001', min: '0.001', max: '10000', required: true };
    const per = h('input', { ...usd, value: '1' }), total = h('input', { ...usd, value: '20' });
    const days = h('input', { type: 'number', min: '1', max: '365', value: '7', required: true });
    const nextWeek = new Date(Date.now() + 7 * 864e5);
    const dl = h('input', { type: 'datetime-local', value: new Date(nextWeek.getTime() - nextWeek.getTimezoneOffset() * 6e4).toISOString().slice(0, 16), required: true });
    const createBatch = h('button', { class: 'primary', type: 'submit' }, CL.create);
    const form = h('form', {}, h('label', {}, CL.vendor, vendor), h('label', {}, CL.models, models), h('label', {}, CL.key, key), h('label', {}, CL.per, per),
      h('label', {}, CL.total, total), h('label', {}, CL.days, days), h('label', {}, CL.deadline, dl), h('p', {}, createBatch));
    form.addEventListener('submit', async ev => {
      ev.preventDefault();
      if (createBatch.disabled) return;
      error.textContent = '';
      createBatch.disabled = true;
      try {
        const request = api('POST', base, { vendor: vendor.value, models: models.value.split(',').map(x => x.trim()).filter(Boolean), api_key: key.value,
          per_coupon_usd: Number(per.value), total_usd: Number(total.value), validity_days: Number(days.value), claim_deadline: new Date(dl.value).toISOString() });
        key.value = '';
        await request;
        if (box.isConnected) await refresh();
      } catch (e) { fail(error)(e); }
      finally { createBatch.disabled = false; }
    });
    box.append(h('h3', {}, a.name + ', ' + CL.btn), h('p', { class: 'muted' }, CL.intro), listBox, out, form, error,
      h('p', {}, h('button', { onclick: () => replacePanel() }, CL.close)));
    async function refresh() {
      try {
        const bs = await api('GET', base);
        if (!box.isConnected) return;
        listBox.replaceChildren(...(bs.length ? bs.map(b => {
          const claim = '/coupon/' + encodeURIComponent(b.coupon_code);
          return h('div', { class: 'row' },
            h('div', {}, h('b', {}, b.coupon_code), b.revoked_at ? h('span', { class: 'chip' }, CL.revoked) : b.stopped_at ? h('span', { class: 'chip' }, CL.stopped) : null,
              h('div', { class: 'muted' }, b.vendor + ', ' + b.models.join(', ')),
              h('div', { class: 'muted' }, couponMoney(b.per_coupon_usd) + ' / ' + couponMoney(b.total_usd) + ', ' + CL.avail + ' ' + couponMoney(b.available_usd) + ', ' + (b.email_enabled ? CL.email : CL.noEmail)),
              h('div', { class: 'muted' }, CL.deadline + ' ' + when(b.claim_deadline)),
              h('details', {}, h('summary', {}, CL.qr), h('img', { class: 'qr', src: claim + '/qr.svg', alt: CL.qr + ' ' + b.coupon_code, loading: 'lazy' }),
                h('a', { href: claim }, CL.link))),
            h('div', {}, h('button', { onclick: async ev => {
              const button = ev.currentTarget;
              try { await navigator.clipboard.writeText(b.claim_url); button.textContent = CL.copied; } catch { error.textContent = CL.copyFailed; }
            } }, CL.copy), ' ', h('button', { onclick: () => rpt(b) }, CL.report), ' ',
            b.revoked_at ? null : h('button', { class: 'danger', onclick: () => rev(b) }, CL.revoke)));
        }) : [h('p', { class: 'muted' }, CL.none)]));
      } catch (e) { fail(error)(e); }
    }
    async function rev(b) {
      if (!confirm(CL.revokeAsk)) return;
      try {
        await api('DELETE', base + '/' + encodeURIComponent(b.id));
        reportVersion++;
        out.replaceChildren();
        await refresh();
      } catch (e) { fail(error)(e); }
    }
    async function rpt(b) {
      const version = ++reportVersion;
      error.textContent = '';
      try {
        const r = await api('GET', base + '/' + encodeURIComponent(b.id) + '/report');
        if (box.isConnected && version === reportVersion) out.replaceChildren(couponReport(b.coupon_code, r));
      } catch (e) { if (version === reportVersion) fail(error)(e); }
    }
    replacePanel(box); box.scrollIntoView({ behavior: 'smooth' }); refresh();
  }
  async function report(a, days) {
    err.textContent = '';
    try {
      const r = await api('GET', '/v1/me/apps/' + encodeURIComponent(a.id) + '/report?days=' + days);
      replacePanel(reportView(a, r, days, d => report(a, d), notice, secretBox, err));
      rep.scrollIntoView({ behavior: 'smooth' });
    } catch (e) { fail(err)(e); }
  }
  load();
}

function couponReport(code, r) {
  const m = r.money;
  const stats = entries => h('div', { class: 'grid' }, entries.map(([label, value]) => h('div', { class: 'stat' }, h('b', {}, value), label)));
  return h('section', {}, h('h3', {}, CL.report + ', ' + code),
    stats([[CL.claimed, r.coupons.claimed], [CL.firstSuccess, r.first_successful_users], [CL.spent, couponMoney(m.spent_usd)],
      [CL.actual, couponMoney(m.actual_cost_usd)], [CL.estimated, couponMoney(m.completed_estimated_cost_usd)],
      [CL.held, couponMoney(m.held_usd)], [CL.uncertain, couponMoney(m.uncertain_held_usd)],
      [CL.returned, couponMoney(m.returned_usd)], [CL.overshoot, couponMoney(m.overshoot_usd)]]),
    h('h4', {}, CL.outcomes), stats(Object.entries(CL.status).map(([status, label]) => [label, r.requests[status]])),
    h('h4', {}, CL.modelSpend), r.models.length ? r.models.map(model => h('div', { class: 'row' },
      h('div', {}, h('b', {}, model.model), h('div', { class: 'muted' }, CL.settled + ' ' + model.requests)),
      h('div', {}, h('b', {}, couponMoney(model.cost_usd)),
        h('div', { class: 'muted' }, CL.actual + ' ' + couponMoney(model.actual_cost_usd)),
        h('div', { class: 'muted' }, CL.estimated + ' ' + couponMoney(model.completed_estimated_cost_usd)))))
      : h('p', { class: 'muted' }, CL.noModels));
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
