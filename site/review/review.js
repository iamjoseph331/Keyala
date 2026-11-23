'use strict';
const server = 'https://keyala-review.onrender.com';
const $ = id => document.getElementById(id);
const pause = () => new Promise(resolve => setTimeout(resolve, 1500));
let grant, busy = false;
const show = text => { $('status').textContent = text; };
function controls() { $('connect').disabled = busy; $('send').disabled = busy || !grant; }
async function call(path, {token, body, headers = {}} = {}) {
  const response = await fetch(server + path, {
    method: body ? 'POST' : 'GET', signal: AbortSignal.timeout(90000),
    headers: {...(body ? {'Content-Type': 'application/json'} : {}), ...(token ? {Authorization: 'Bearer ' + token} : {}), ...headers},
    ...(body ? {body: JSON.stringify(body)} : {})
  });
  const json = await response.json();
  if (!response.ok) throw new Error(json.error?.message || json.error?.type || `HTTP ${response.status}`);
  return {status: response.status, json};
}
$('connect').onclick = async () => {
  if (busy) return;
  busy = true; grant = undefined; controls(); $('pair').hidden = true;
  show('Creating a code. The review server may need a minute to wake up…');
  try {
    const {json: c} = await call('/v1/connect', {body: {app_id: 'app_review', vendor: 'openai'}});
    $('code').textContent = c.connect_code;
    $('qr').src = server + '/connect/' + encodeURIComponent(c.connect_code) + '/qr.svg';
    // A custom URL opens the app; the reviewer has already selected the explicit review server.
    $('open').href = 'keyala://connect/' + encodeURIComponent(c.connect_code);
    $('pair').hidden = false; show('Waiting for connection approval on the iPhone.');
    const deadline = Date.now() + 10 * 60 * 1000;
    while (Date.now() < deadline) {
      await pause();
      const {json: result} = await call('/v1/connect/' + encodeURIComponent(c.connect_code), {token: c.connect_secret});
      if (result.status === 'approved' && result.grant_token) {
        grant = result.grant_token; $('pair').hidden = true;
        show('Connected. Send one mock request, then open Inbox on the iPhone.'); return;
      }
      if (['denied', 'expired'].includes(result.status)) throw new Error('Connection ' + result.status + '. Create a new code to try again.');
    }
    throw new Error('Connection expired. Create a new code.');
  } catch (error) { show(error.message); }
  finally { busy = false; controls(); }
};
$('send').onclick = async () => {
  if (busy || !grant) return;
  busy = true; controls(); show('Sending once. Open Inbox on the iPhone to allow or deny.');
  try {
    let {status, json} = await call('/v1/openai/chat/completions', {
      token: grant, headers: {'Idempotency-Key': crypto.randomUUID(), 'X-Keyala-Wait': '1'},
      body: {model: 'gpt-4o-mini', max_tokens: 32, messages: [{role: 'user', content: 'Say hello to the Keyala reviewer.'}]}
    });
    if (status === 202) {
      const id = json.request_id, deadline = Date.now() + 180000;
      show('Waiting for the iPhone decision. This request has not been retried.');
      while (Date.now() < deadline) {
        await pause();
        ({json} = await call('/v1/requests/' + encodeURIComponent(id), {token: grant}));
        if (!['pending', 'approved', 'sending'].includes(json.status)) break;
      }
    }
    show(JSON.stringify(json, null, 2));
  } catch (error) { show(error.message + '\nNo automatic retry. Check Activity in Keyala before sending again.'); }
  finally { busy = false; controls(); }
};
