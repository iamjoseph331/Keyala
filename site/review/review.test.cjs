const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const elements = new Map();
const element = id => {
  if (!elements.has(id)) elements.set(id, {});
  return elements.get(id);
};
let posts = 0, release, fail = false;
const context = vm.createContext({
  document: {getElementById: element},
  setTimeout: resolve => resolve(), Date, AbortSignal,
  crypto: require('node:crypto').webcrypto,
  fetch: async (url, options) => {
    assert.ok(url.startsWith('https://keyala-review.onrender.com/'));
    let status = 200, json;
    if (url.endsWith('/v1/connect')) json = {connect_code: 'KY-TEST', connect_secret: 'test-secret'};
    else if (url.includes('/v1/connect/')) json = {status: 'approved', grant_token: 'test-grant'};
    else if (options.method === 'POST') {
      posts++;
      assert.equal(options.headers.Authorization, 'Bearer test-grant');
      if (fail) throw new Error('network unavailable');
      await new Promise(resolve => { release = resolve; });
      status = 202; json = {request_id: 'test-request'};
    } else json = {status: 'completed', response: {mock: true}};
    return {ok: true, status, json: async () => json};
  }
});
vm.runInContext(fs.readFileSync(__dirname + '/review.js', 'utf8'), context);
(async () => {
  await element('connect').onclick();
  assert.equal(element('send').disabled, false);
  assert.equal(element('open').href, 'keyala://connect/KY-TEST');
  const sending = element('send').onclick();
  assert.equal(element('send').disabled, true);
  await element('send').onclick();
  assert.equal(posts, 1, 'Double clicks must not submit a second request');
  release(); await sending;
  assert.match(element('status').textContent, /completed/);
  fail = true; await element('send').onclick();
  assert.equal(posts, 2, 'A network failure must not retry');
  assert.match(element('status').textContent, /No automatic retry/);
  console.log('Review tool checks passed: isolated origin, pairing, held request completion, duplicate-click guard, no retry on network failure.');
})().catch(error => { console.error(error); process.exitCode = 1; });
