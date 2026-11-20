/* Keyala web SDK. Served by each Keyala server at /sdk/keyala.js and talks to that same server.
 *
 *   <script src="https://app.keyala.io/sdk/keyala.js"></script>
 *   const { grant_token } = await Keyala.connect({ app: "app_...", vendor: "openai" });
 *   // then call {server}/v1/openai/chat/completions with Authorization: Bearer grant_token
 *
 * or a ready-made button that opens the same dialog:
 *   <keyala-connect app="app_..." vendor="openai"></keyala-connect>   -> "connected" event, detail = { grant_token, vendor }
 *
 * Browser apps need their page origin in the app's allowed_origins. The SDK never stores the token; keep it as you see fit.
 * Errors reject with err.code: "cancelled" | "denied" | "unreachable" | "failed".
 */
(() => {
  if (window.Keyala) return;
  const SCRIPT = document.currentScript && document.currentScript.src;
  const SERVER = SCRIPT ? new URL(SCRIPT).origin : 'https://app.keyala.io';
  const GET_APP = 'https://keyala.io/';
  const VENDOR = { openai: 'OpenAI', anthropic: 'Anthropic', openrouter: 'OpenRouter' };
  const T = {
    en: { title: 'Use your own {v} key', why: 'Approve on your phone with Keyala. Your key stays in Keyala and you approve each call.', scan: 'Scan with your iPhone camera', open: 'Open Keyala', code: 'Or enter this code in the app', wait: 'Waiting for your phone…', app: 'Approve on your phone.', web: 'Your phone opened the link but Keyala is not installed. Install it, sign in, then scan again.', webPhone: 'Keyala is not installed yet. Get it, sign in, then come back and tap Open Keyala again.', get: 'No app yet? Get Keyala', cancel: 'Cancel', expired: 'This code expired.', again: 'New code', denied: 'You declined on your phone.', unreachable: 'Could not reach Keyala. It may be waking up; try again in a minute.', waking: 'Waking up the Keyala server, this can take up to a minute…', btn: 'Connect Keyala', on: 'Keyala connected' },
    'zh-Hant': { title: '使用你自己的 {v} 金鑰', why: '在手機上用 Keyala 核准。金鑰留在 Keyala，每次呼叫都由你核准。', scan: '用 iPhone 相機掃描', open: '打開 Keyala', code: '或在 App 中輸入這組代碼', wait: '等待手機核准…', app: '請在手機上核准。', web: '手機打開了連結，但還沒安裝 Keyala。安裝並登入後再掃一次。', webPhone: '還沒安裝 Keyala。下載並登入後，回到這裡再按一次「打開 Keyala」。', get: '還沒有 App？下載 Keyala', cancel: '取消', expired: '代碼已過期。', again: '產生新代碼', denied: '你在手機上拒絕了。', unreachable: '連不到 Keyala，伺服器可能正在啟動，請稍後再試。', waking: '正在喚醒 Keyala 伺服器，最多約需一分鐘…', btn: '連接 Keyala', on: 'Keyala 已連接' },
    ja: { title: '自分の {v} キーを使う', why: 'スマートフォンの Keyala で承認します。キーは Keyala に保管され、呼び出しごとに承認します。', scan: 'iPhone のカメラでスキャン', open: 'Keyala を開く', code: 'またはアプリでこのコードを入力', wait: 'スマートフォンでの承認を待っています…', app: 'スマートフォンで承認してください。', web: 'リンクは開かれましたが、Keyala がインストールされていません。インストールしてサインインし、もう一度スキャンしてください。', webPhone: 'Keyala がまだインストールされていません。入手してサインインしたら、戻って「Keyala を開く」をもう一度タップしてください。', get: 'アプリをお持ちでない方：Keyala を入手', cancel: 'キャンセル', expired: 'コードの有効期限が切れました。', again: '新しいコード', denied: 'スマートフォンで拒否されました。', unreachable: 'Keyala に接続できません。起動中の可能性があります。少し待ってからお試しください。', waking: 'Keyala サーバーを起動しています。最大1分ほどかかります…', btn: 'Keyala を接続', on: 'Keyala 接続済み' },
  };
  const pickLang = (l) => { l = (l || navigator.language || 'en').toLowerCase(); return l.startsWith('zh') ? 'zh-Hant' : l.startsWith('ja') ? 'ja' : 'en'; };
  // Same-device pairing: a QR is useless on the phone that would scan it.
  const isPhone = () => /iPhone|iPod|Android.+Mobile/i.test(navigator.userAgent);
  const fail = (code, message) => Object.assign(new Error(message || code), { code });
  const esc = (s) => String(s).replace(/[&<>"']/g, (c) => '&#' + c.charCodeAt(0) + ';');

  const CSS = `
    :host { all: initial; }
    .shade { position: fixed; inset: 0; z-index: 2147483000; background: rgba(20,18,15,.5); display: grid; place-items: center; padding: 16px; font: 15px/1.45 -apple-system, system-ui, Inter, sans-serif; }
    .box { --bg:#FAF7F2; --ink:#1F1B16; --mut:#7A7268; --acc:#3D7A57; --line:#ECE6DC; --warn:#A8662F;
      background: var(--bg); color: var(--ink); border-radius: 16px; padding: 22px; width: 100%; max-width: 340px; box-sizing: border-box;
      display: grid; gap: 12px; justify-items: center; text-align: center; box-shadow: 0 12px 40px rgba(0,0,0,.25); }
    @media (prefers-color-scheme: dark) { .box { --bg:#211E1A; --ink:#F1ECE4; --mut:#A59D92; --acc:#6DB58A; --line:#2E2A25; --warn:#E0A36B; } }
    .logo { width: 40px; height: 40px; border-radius: 11px; background: #3D7A57; color: #FAF7F2; font: 700 15px system-ui; display: grid; place-items: center; }
    h2 { font-size: 18px; margin: 0; } p { margin: 0; } .mut { color: var(--mut); font-size: 13px; }
    img { width: 188px; height: 188px; background: #fff; border-radius: 10px; padding: 8px; }
    .code { font: 600 17px ui-monospace, Menlo, monospace; letter-spacing: .06em; }
    .st { font-size: 13px; color: var(--mut); display: flex; gap: 8px; align-items: center; justify-content: center; }
    .st.warn { color: var(--warn); font-weight: 600; }
    .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--warn); flex: none; animation: p 1.2s infinite; }
    @keyframes p { 50% { opacity: .3 } } @media (prefers-reduced-motion: reduce) { .dot { animation: none } }
    a.open, button { font: 600 15px system-ui; border: 0; border-radius: 10px; padding: 12px 16px; cursor: pointer; text-decoration: none; }
    a.open, button.primary { background: var(--acc); color: #fff; width: 100%; box-sizing: border-box; display: block; }
    button.plain { background: transparent; color: var(--mut); padding: 6px 10px; }
    a.get { color: var(--acc); font-size: 13px; }
    :focus-visible { outline: 2px solid var(--acc); outline-offset: 2px; }`;

  async function call(server, path, opts = {}) {
    let r;
    try { r = await fetch(server + path, opts); } catch { throw fail('unreachable'); }
    const j = await r.json().catch(() => ({}));
    if (!r.ok) throw fail('failed', (j.error && j.error.message) || 'Keyala ' + r.status);
    return j;
  }

  /** Opens the pairing dialog; resolves { grant_token, vendor } once the user approves on their phone. */
  function connect({ app, vendor, server = SERVER, lang } = {}) {
    if (!app || !VENDOR[vendor]) return Promise.reject(fail('failed', 'Keyala.connect needs { app, vendor: "openai" | "anthropic" | "openrouter" }'));
    server = server.replace(/\/$/, '');
    const t = T[pickLang(lang)], phone = isPhone();
    const host = document.createElement('div');
    const root = host.attachShadow({ mode: 'open' });
    root.innerHTML = `<style>${CSS}</style><div class="shade"><div class="box" role="dialog" aria-modal="true" aria-labelledby="t">
      <div class="logo" aria-hidden="true">🐨</div><h2 id="t">${esc(t.title.replace('{v}', VENDOR[vendor]))}</h2><p class="mut">${esc(t.why)}</p>
      <div id="body" style="display:grid;gap:10px;justify-items:center;width:100%"><div class="st"><span class="dot"></span>…</div></div>
      <a class="get" href="${GET_APP}" target="_blank" rel="noopener">${esc(t.get)}</a>
      <button class="plain" id="x">${esc(t.cancel)}</button></div></div>`;
    const $ = (id) => root.getElementById(id);
    const prevFocus = document.activeElement;
    document.body.appendChild(host);
    $('x').focus();

    return new Promise((resolve, reject) => {
      let gen = 0, done = false; // gen guards against a stale poll loop after "New code"
      const finish = (err, val) => {
        if (done) return; done = true; gen++;
        host.remove(); document.removeEventListener('keydown', onKey, true);
        if (prevFocus && prevFocus.focus) prevFocus.focus();
        err ? reject(err) : resolve(val);
      };
      const onKey = (e) => { if (e.key === 'Escape') { e.stopPropagation(); finish(fail('cancelled')); } };
      document.addEventListener('keydown', onKey, true);
      $('x').onclick = () => finish(fail('cancelled'));
      const status = (text, warn) => `<div class="st${warn ? ' warn' : ''}">${warn ? '' : '<span class="dot"></span>'}${esc(text)}</div>`;
      const showEnd = (text) => {
        $('body').innerHTML = `${status(text, true)}<button class="primary" id="again">${esc(t.again)}</button>`;
        $('again').onclick = start;
      };

      async function start() {
        const me = ++gen;
        $('body').innerHTML = status('…');
        let c;
        const slow = setTimeout(() => { if (me === gen) $('body').innerHTML = status(t.waking); }, 3000); // free hosting sleeps
        try { c = await call(server, '/v1/connect', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ app_id: app, vendor }) }); }
        catch (e) { if (me === gen) e.code === 'unreachable' ? showEnd(t.unreachable) : finish(e); return; }
        finally { clearTimeout(slow); }
        if (me !== gen) return;
        $('body').innerHTML = phone
          ? `<a class="open" href="${esc(c.connect_url)}">${esc(t.open)}</a><p class="mut">${esc(t.code)}</p><div class="code">${esc(c.connect_code)}</div><div id="st">${status(t.wait)}</div>`
          : `<p class="mut">${esc(t.scan)}</p><img alt="QR ${esc(c.connect_code)}" src="${esc(c.connect_qr_url)}"><p class="mut">${esc(t.code)}</p><div class="code">${esc(c.connect_code)}</div><div id="st">${status(t.wait)}</div>`;
        for (;;) {
          await new Promise((r) => setTimeout(r, 1500));
          if (me !== gen) return;
          let s;
          try { s = await call(server, '/v1/connect/' + encodeURIComponent(c.connect_code), { headers: { Authorization: 'Bearer ' + c.connect_secret } }); }
          catch { continue; } // keep waiting through a blip; the code expires on its own
          if (me !== gen) return;
          if (s.status === 'approved' && s.grant_token) return finish(null, { grant_token: s.grant_token, vendor });
          if (s.status === 'approved') return finish(fail('failed', 'Approved, but the token was already collected. Connect again.'));
          if (s.status === 'denied') return finish(fail('denied', t.denied));
          if (s.status !== 'pending') return showEnd(t.expired);
          $('st').innerHTML = s.seen === 'app' ? status(t.app) : s.seen === 'web' ? status(phone ? t.webPhone : t.web, true) : status(t.wait);
        }
      }
      start();
    });
  }

  class Button extends HTMLElement {
    connectedCallback() {
      if (this.shadowRoot) return;
      const root = this.attachShadow({ mode: 'open' });
      const t = T[pickLang(this.getAttribute('lang'))];
      root.innerHTML = `<style>button{font:600 15px system-ui;border:0;border-radius:10px;padding:10px 14px;cursor:pointer;background:#3D7A57;color:#fff;display:inline-flex;gap:8px;align-items:center}
        button[disabled]{opacity:.85;cursor:default;background:#EFEAE1;color:#3D7A57}i{width:18px;height:18px;border-radius:5px;background:#FAF7F2;color:#3D7A57;font:12px system-ui;display:grid;place-items:center;font-style:normal}
        :focus-visible{outline:2px solid #3D7A57;outline-offset:2px}</style><button part="button"><i>🐨</i><span></span></button>`;
      const b = root.querySelector('button'), label = root.querySelector('span');
      const render = () => { const on = this.hasAttribute('connected'); b.disabled = on; label.textContent = on ? '✓ ' + t.on : t.btn; };
      render();
      new MutationObserver(render).observe(this, { attributes: true, attributeFilter: ['connected'] });
      b.onclick = async () => {
        try {
          const g = await connect({ app: this.getAttribute('app'), vendor: this.getAttribute('vendor'), server: this.getAttribute('server') || SERVER, lang: this.getAttribute('lang') });
          this.setAttribute('connected', '');
          this.dispatchEvent(new CustomEvent('connected', { detail: g, bubbles: true }));
        } catch (e) { this.dispatchEvent(new CustomEvent('error', { detail: e, bubbles: true })); }
      };
    }
  }
  if (!customElements.get('keyala-connect')) customElements.define('keyala-connect', Button);
  window.Keyala = { connect, server: SERVER };
})();
