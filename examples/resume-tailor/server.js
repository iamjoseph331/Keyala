// Resume Tailor: an AI app with NO API key of its own. Each visitor lends their own OpenAI or
// Anthropic key through MasterKey and approves every call on their phone, so the person who
// publishes this app never pays for (or holds) anyone's tokens.
//   MK_URL=http://localhost:8080 MK_APP_SECRET=mk_app_... node server.js
const http = require('node:http');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const OpenAI = require('openai');
const Anthropic = require('@anthropic-ai/sdk');

const MK_URL = (process.env.MK_URL || 'http://localhost:8080').replace(/\/$/, '');
const MK_APP_SECRET = process.env.MK_APP_SECRET;
const PORT = Number(process.env.PORT || 3001);
const HOST = process.env.HOST || '127.0.0.1';
if (!MK_APP_SECRET) {
  console.error('MK_APP_SECRET is required. Get one with:\n  curl -s -X POST $MK_URL/v1/apps -d \'{"name":"Resume Tailor"}\'');
  process.exit(1);
}

// Must match the MasterKey server's allowlist (backend/src/pricing.rs). Prices: USD per 1M tokens, display only.
const MODELS = [
  { id: 'claude-sonnet-5-5', vendor: 'anthropic', label: 'Claude Sonnet 5.5', price: '$2 / $10' },
  { id: 'claude-opus-5-5', vendor: 'anthropic', label: 'Claude Opus 5.5', price: '$4 / $20' },
  { id: 'claude-haiku-5-5', vendor: 'anthropic', label: 'Claude Haiku 5.5', price: '$0.10 / $0.50' },
  { id: 'gpt-4.1', vendor: 'openai', label: 'GPT-4.1', price: '$2 / $8' },
  { id: 'gpt-4.1-mini', vendor: 'openai', label: 'GPT-4.1 mini', price: '$0.40 / $1.60' },
  { id: 'gpt-4o', vendor: 'openai', label: 'GPT-4o', price: '$2.50 / $10' },
  { id: 'gpt-4o-mini', vendor: 'openai', label: 'GPT-4o mini', price: '$0.15 / $0.60' },
];
const MAX_TOKENS = 4096; // the server's per-model output cap; a full resume fits
const LIMIT = 60_000; // chars per text field

// ponytail: sessions live in memory (lost on restart, users just reconnect). Use a real store when you scale out.
// sid -> { grants: {openai?, anthropic?}, pending: {vendor: {code,url,expires_at}} }
const sessions = new Map();
function session(req, res) {
  const sid = /(?:^|;\s*)sid=([\w-]{36})/.exec(req.headers.cookie || '')?.[1];
  if (sid && sessions.has(sid)) return sessions.get(sid);
  const id = crypto.randomUUID();
  const s = { grants: {}, pending: {} };
  sessions.set(id, s);
  res.setHeader('Set-Cookie', `sid=${id}; HttpOnly; SameSite=Lax; Path=/`);
  return s;
}

const mk = (p, opts = {}) =>
  fetch(MK_URL + p, { ...opts, headers: { Authorization: `Bearer ${MK_APP_SECRET}`, 'Content-Type': 'application/json' } });
const send = (res, status, body) => {
  res.writeHead(status, { 'Content-Type': 'application/json' });
  res.end(JSON.stringify(body));
};
const readJson = (req) =>
  new Promise((ok, fail) => {
    let b = '';
    req.on('data', (c) => { b += c; if (b.length > 1_000_000) { fail(new Error('request too large')); req.destroy(); } });
    req.on('end', () => { try { ok(JSON.parse(b || '{}')); } catch { ok({}); } });
  });
const text = (v) => String(v ?? '').slice(0, LIMIT).trim();

// The app's actual value: the prompt. It stays on the server; the user still sees it on their phone before approving.
function prompt({ resume, jd, language, tone, extra }) {
  const system = [
    'You are an expert resume writer and recruiter.',
    'Rewrite the candidate\'s resume so it is tailored to the job description.',
    'Rules: never invent employers, titles, dates, degrees, metrics or skills. Use only facts from the resume and the',
    'additional experiences. Reorder, rephrase and emphasise what matches the job; mirror the job\'s key terms where',
    'they truthfully apply; drop or shorten what is irrelevant. Keep it ATS-friendly: plain Markdown, one "#" name',
    'heading, "##" section headings, "-" bullets, no tables, no images, no commentary before or after the resume.',
    `Tone: ${tone}.`,
    language ? `Write the entire resume in ${language}.` : 'Write the resume in the same language as the original resume.',
  ].join(' ');
  const user = [
    `<resume>\n${resume}\n</resume>`,
    extra ? `<additional_experiences>\n${extra}\n</additional_experiences>` : '',
    `<job_description>\n${jd}\n</job_description>`,
  ].filter(Boolean).join('\n\n');
  return { system, user };
}

const failure = (status, type, message) => {
  if (status === 403 && type === 'masterkey_denied') return { kind: 'denied', message: 'You denied this request on your phone.' };
  if (status === 410) return { kind: 'expired', message: 'The request expired before you approved it.' };
  if (status === 401) return { kind: 'disconnected', message: 'The connection was revoked. Connect again.' };
  return { kind: 'error', message: message || `MasterKey returned ${status}` };
};

async function tailor(s, model, input, idemKey) {
  const grant = s.grants[model.vendor];
  const { system, user } = prompt(input);
  // >>> THE ONLY MASTERKEY-SPECIFIC PART: the official SDKs, pointed at MasterKey, with the user's grant
  // >>> token where an API key would go. maxRetries: 0 so a held/uncertain call is never re-sent.
  const opts = { apiKey: grant, maxRetries: 0, timeout: 60_000, defaultHeaders: { 'X-MasterKey-Wait': '25' } };
  const reqOpts = { headers: { 'Idempotency-Key': idemKey } };
  try {
    if (model.vendor === 'openai') {
      const client = new OpenAI({ ...opts, baseURL: MK_URL + '/v1/openai' });
      const r = await client.chat.completions.create(
        { model: model.id, max_tokens: MAX_TOKENS, messages: [{ role: 'system', content: system }, { role: 'user', content: user }] }, reqOpts);
      if (!r.choices) return { pending: true, request_id: r.request_id }; // 202: still waiting for the phone
      return { text: r.choices[0].message.content, request_id: r.id };
    }
    const client = new Anthropic({ ...opts, baseURL: MK_URL + '/v1/anthropic' });
    const r = await client.messages.create({ model: model.id, max_tokens: MAX_TOKENS, system, messages: [{ role: 'user', content: user }] }, reqOpts);
    if (!r.content) return { pending: true, request_id: r.request_id };
    return { text: r.content.map((c) => c.text || '').join(''), request_id: r.id };
  } catch (e) {
    const err = failure(e.status, e?.error?.error?.type || e?.error?.type, e?.error?.error?.message || e.message);
    if (err.kind === 'disconnected') delete s.grants[model.vendor];
    return { error: err };
  }
}

const routes = {
  'GET /': (req, res) => {
    res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
    res.end(fs.readFileSync(path.join(__dirname, 'index.html')));
  },
  'GET /api/state': (req, res, s) =>
    send(res, 200, { models: MODELS, connected: { openai: !!s.grants.openai, anthropic: !!s.grants.anthropic } }),
  'POST /api/connect': async (req, res, s) => {
    const { vendor } = await readJson(req);
    if (vendor !== 'openai' && vendor !== 'anthropic') return send(res, 400, { error: 'unknown vendor' });
    const r = await mk('/v1/connect', { method: 'POST', body: JSON.stringify({ vendor, suggested_policy: { mode: 'ask' } }) });
    const j = await r.json();
    if (!r.ok) return send(res, 502, { error: j.error?.message || 'MasterKey refused to start a connection' });
    s.pending[vendor] = { code: j.connect_code, url: j.connect_url, expires_at: j.expires_at };
    send(res, 200, s.pending[vendor]);
  },
  'GET /api/connect/status': async (req, res, s, url) => {
    const vendor = url.searchParams.get('vendor');
    if (s.grants[vendor]) return send(res, 200, { status: 'approved' });
    const p = s.pending[vendor];
    if (!p) return send(res, 200, { status: 'none' });
    const j = await (await mk(`/v1/connect/${encodeURIComponent(p.code)}`)).json();
    if (j.status === 'approved' && j.grant_token) { s.grants[vendor] = j.grant_token; delete s.pending[vendor]; }
    if (j.status !== 'pending' && j.status !== 'approved') delete s.pending[vendor];
    send(res, 200, { status: s.grants[vendor] ? 'approved' : j.status });
  },
  'POST /api/tailor': async (req, res, s) => {
    const b = await readJson(req);
    const model = MODELS.find((m) => m.id === b.model);
    if (!model) return send(res, 400, { error: { kind: 'error', message: 'Pick a model.' } });
    if (!s.grants[model.vendor]) return send(res, 200, { error: { kind: 'disconnected', message: 'Connect MasterKey first.' } });
    const input = { resume: text(b.resume), jd: text(b.jd), extra: text(b.extra), language: text(b.language).slice(0, 40), tone: text(b.tone).slice(0, 40) || 'professional' };
    if (!input.resume || !input.jd) return send(res, 400, { error: { kind: 'error', message: 'A resume and a job description are required.' } });
    send(res, 200, await tailor(s, model, input, String(b.key || crypto.randomUUID()).slice(0, 128)));
  },
  'GET /api/request': async (req, res, s, url) => {
    const grant = s.grants[url.searchParams.get('vendor')];
    if (!grant) return send(res, 200, { error: failure(401) });
    const r = await fetch(`${MK_URL}/v1/requests/${encodeURIComponent(url.searchParams.get('id') || '')}`, { headers: { Authorization: `Bearer ${grant}` } });
    const j = await r.json();
    if (!r.ok) return send(res, 200, { error: failure(r.status, j.error?.type, j.error?.message) });
    if (j.status === 'completed') {
      const out = j.response?.choices?.[0]?.message?.content ?? (j.response?.content || []).map((c) => c.text || '').join('');
      return send(res, 200, { text: out });
    }
    if (j.status === 'denied') return send(res, 200, { error: failure(403, 'masterkey_denied') });
    if (j.status === 'expired') return send(res, 200, { error: failure(410) });
    if (j.status === 'failed' || j.status === 'uncertain') return send(res, 200, { error: { kind: 'error', message: `Request ${j.status}.` } });
    send(res, 200, { pending: true });
  },
};

http.createServer(async (req, res) => {
  const url = new URL(req.url, 'http://x');
  const h = routes[`${req.method} ${url.pathname}`];
  if (!h) return send(res, 404, { error: 'not found' });
  try { await h(req, res, session(req, res), url); } catch (e) { send(res, 500, { error: { kind: 'error', message: String(e.message || e) } }); }
}).listen(PORT, HOST, () => console.log(`Resume Tailor: http://localhost:${PORT}  (MasterKey at ${MK_URL})`));
