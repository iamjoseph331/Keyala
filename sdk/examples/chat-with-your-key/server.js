// "Chat with your own key": a demo app that has NO API key of its own.
// Users lend their key through MasterKey and approve each call on their phone.
//   MK_URL=http://localhost:8080 MK_APP_SECRET=mk_app_... node server.js
const http = require('node:http');
const fs = require('node:fs');
const path = require('node:path');
const OpenAI = require('openai');

const MK_URL = (process.env.MK_URL || 'http://localhost:8080').replace(/\/$/, '');
const MK_APP_SECRET = process.env.MK_APP_SECRET;
const PORT = Number(process.env.PORT || 3000);
const MODEL = process.env.MODEL || 'gpt-4o-mini';
if (!MK_APP_SECRET) {
  console.error('MK_APP_SECRET is required. Get one with:\n  curl -s -X POST $MK_URL/v1/apps -d \'{"name":"Chat with your key"}\'');
  process.exit(1);
}

// ponytail: one demo user, state lives in memory. Real apps store the grant token per user.
let grantToken = null;
let pending = null; // {code, url, expires_at}

const mk = (p, opts = {}) =>
  fetch(MK_URL + p, { ...opts, headers: { Authorization: `Bearer ${MK_APP_SECRET}`, 'Content-Type': 'application/json' } });

const send = (res, status, body) => {
  res.writeHead(status, { 'Content-Type': 'application/json' });
  res.end(JSON.stringify(body));
};
const readJson = (req) =>
  new Promise((ok) => {
    let b = '';
    req.on('data', (c) => (b += c));
    req.on('end', () => { try { ok(JSON.parse(b || '{}')); } catch { ok({}); } });
  });

async function chat(messages, idemKey) {
  // >>> THE ONLY MASTERKEY-SPECIFIC LINE: point the official SDK at MasterKey, and use the
  // >>> user's grant token where an API key would go. Everything else is plain OpenAI code.
  const client = new OpenAI({ baseURL: MK_URL + '/v1/openai', apiKey: grantToken, maxRetries: 0, defaultHeaders: { 'X-MasterKey-Wait': '25' } });
  // (maxRetries: 0 on purpose: a held/uncertain call must never be re-sent automatically.)
  try {
    const r = await client.chat.completions.create(
      { model: MODEL, messages, max_tokens: 400 },
      // one key per user message: if anything re-sends it, MasterKey returns the same request instead of a second charge
      { headers: idemKey ? { 'Idempotency-Key': String(idemKey).slice(0, 128) } : {} },
    );
    if (!r.choices) return { pending: true, request_id: r.request_id }; // 202: still waiting for the phone
    return { text: r.choices[0].message.content, request_id: r.id };
  } catch (e) {
    const type = e?.error?.error?.type || e?.error?.type;
    if (e.status === 403 && type === 'masterkey_denied') return { error: { kind: 'denied', message: 'You denied this request on your phone.' } };
    if (e.status === 410) return { error: { kind: 'expired', message: 'The request expired before you approved it.' } };
    if (e.status === 401) { grantToken = null; return { error: { kind: 'disconnected', message: 'The connection was revoked. Connect again.' } }; }
    return { error: { kind: 'error', message: e?.error?.error?.message || e.message } };
  }
}

const routes = {
  'GET /': (req, res) => {
    res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
    res.end(fs.readFileSync(path.join(__dirname, 'index.html')));
  },
  'GET /api/state': (req, res) => send(res, 200, { connected: !!grantToken, pending, model: MODEL }),
  'POST /api/connect': async (req, res) => {
    const r = await mk('/v1/connect', { method: 'POST', body: JSON.stringify({ vendor: 'openai', suggested_policy: { mode: 'ask' } }) });
    const j = await r.json();
    if (!r.ok) return send(res, 502, { error: j.error?.message || 'MasterKey refused to start a connection' });
    pending = { code: j.connect_code, url: j.connect_url, expires_at: j.expires_at };
    send(res, 200, pending);
  },
  'GET /api/connect/status': async (req, res) => {
    if (grantToken) return send(res, 200, { status: 'approved' });
    if (!pending) return send(res, 200, { status: 'none' });
    const j = await (await mk(`/v1/connect/${pending.code}`)).json();
    if (j.status === 'approved' && j.grant_token) { grantToken = j.grant_token; pending = null; }
    if (j.status !== 'pending' && j.status !== 'approved') pending = null;
    send(res, 200, { status: grantToken ? 'approved' : j.status });
  },
  'POST /api/chat': async (req, res) => {
    if (!grantToken) return send(res, 401, { error: { kind: 'disconnected', message: 'Connect MasterKey first.' } });
    const { messages, key } = await readJson(req);
    send(res, 200, await chat(Array.isArray(messages) ? messages : [], key));
  },
  'GET /api/request': async (req, res, url) => {
    const r = await fetch(`${MK_URL}/v1/requests/${encodeURIComponent(url.searchParams.get('id') || '')}`, { headers: { Authorization: `Bearer ${grantToken}` } });
    const j = await r.json();
    if (j.status === 'completed') return send(res, 200, { text: j.response?.choices?.[0]?.message?.content ?? '' });
    if (j.status === 'denied') return send(res, 200, { error: { kind: 'denied', message: 'You denied this request on your phone.' } });
    if (j.status === 'expired') return send(res, 200, { error: { kind: 'expired', message: 'The request expired before you approved it.' } });
    if (j.status === 'failed' || j.status === 'uncertain') return send(res, 200, { error: { kind: 'error', message: `Request ${j.status}.` } });
    send(res, 200, { pending: true });
  },
};

http.createServer(async (req, res) => {
  const url = new URL(req.url, 'http://x');
  const h = routes[`${req.method} ${url.pathname}`];
  if (!h) return send(res, 404, { error: 'not found' });
  try { await h(req, res, url); } catch (e) { send(res, 500, { error: String(e.message || e) }); }
}).listen(PORT, () => console.log(`Chat with your key: http://localhost:${PORT}  (MasterKey at ${MK_URL})`));
