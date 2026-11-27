import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { runInNewContext } from 'node:vm';

const script = readFileSync(new URL('../src/keyala.js', import.meta.url), 'utf8');
const app = 'app_fixture';
const vendor = 'openai';
const coupon = 'KC-aBcDeFgHiJkLmNoPqRsTuA';
const grant = 'keyala_grant_fixture';

function sdk(rejectConnect = false) {
  const requests = [];
  const window = {};
  const customElements = new Map();
  class Element {
    attributes = new Map();
    events = [];
    attachShadow() {
      const nodes = new Map();
      const node = key => {
        if (!nodes.has(key)) nodes.set(key, { focus() {} });
        return nodes.get(key);
      };
      this.shadowRoot = { getElementById: node, querySelector: node };
      return this.shadowRoot;
    }
    hasAttribute(name) { return this.attributes.has(name); }
    getAttribute(name) { return this.attributes.get(name) ?? null; }
    setAttribute(name, value) { this.attributes.set(name, value); }
    dispatchEvent(event) { this.events.push(event); }
    remove() {}
  }
  customElements.define = customElements.set;
  runInNewContext(script, {
    window, customElements, navigator: { language: 'en', userAgent: '' }, HTMLElement: Element,
    document: { createElement: () => new Element(), body: { appendChild() {} }, addEventListener() {}, removeEventListener() {} },
    MutationObserver: class { observe() {} },
    CustomEvent: class { constructor(type, { detail }) { this.type = type; this.detail = detail; } },
    setTimeout: (callback, wait) => { if (wait === 1500) queueMicrotask(callback); }, clearTimeout() {},
    fetch: async (url, options) => {
      requests.push({ url, options });
      if (options.method === 'POST') {
        return rejectConnect
          ? { ok: false, status: 410, json: async () => ({ error: { message: 'Coupon expired' } }) }
          : { ok: true, json: async () => ({ connect_code: 'KY-FIXTURE', connect_secret: 'fixture' }) };
      }
      return { ok: true, json: async () => ({ status: 'approved', grant_token: grant }) };
    },
  });
  function button(couponAttr) {
    const el = new (customElements.get('keyala-connect'))();
    el.setAttribute('app', app);
    el.setAttribute('vendor', vendor);
    if (couponAttr !== undefined) el.setAttribute('coupon', couponAttr);
    el.connectedCallback();
    return el;
  }
  return { connect: window.Keyala.connect, requests, button };
}

test('connect sends the explicit coupon unchanged and ordinary BYOK omits coupon_code', async () => {
  const { connect, requests } = sdk();
  assert.equal((await connect({ app, vendor, coupon })).grant_token, grant);
  assert.equal((await connect({ app, vendor })).grant_token, grant);
  assert.deepEqual(requests.filter(r => r.options.method === 'POST').map(r => JSON.parse(r.options.body)), [
    { app_id: app, vendor, coupon_code: coupon },
    { app_id: app, vendor },
  ]);
});

test('a rejected coupon terminates without starting a personal-key connect', async () => {
  const { connect, requests } = sdk(true);
  await assert.rejects(connect({ app, vendor, coupon }), { message: 'Coupon expired' });
  assert.equal(requests.length, 1);
  assert.equal(JSON.parse(requests[0].options.body).coupon_code, coupon);
  for (const bad of ['', ' ', null, undefined, 12]) {
    await assert.rejects(connect({ app, vendor, coupon: bad }), { code: 'failed' });
  }
  assert.equal(requests.length, 1);
});

test('the connect element forwards coupon and fails closed for an empty attribute', async () => {
  const { button, requests } = sdk();
  const sponsored = button(coupon), personal = button(), invalid = button('');
  for (const el of [sponsored, personal, invalid]) await el.shadowRoot.querySelector('button').onclick();
  assert.equal(sponsored.events[0].detail.grant_token, grant);
  assert.equal(personal.events[0].detail.grant_token, grant);
  assert.equal(invalid.events[0].type, 'error');
  assert.deepEqual(requests.filter(r => r.options.method === 'POST').map(r => JSON.parse(r.options.body)), [
    { app_id: app, vendor, coupon_code: coupon },
    { app_id: app, vendor },
  ]);
});
