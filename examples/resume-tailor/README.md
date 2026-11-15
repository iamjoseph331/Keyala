# Resume Tailor

A one-page AI app that tailors a resume to one or more job descriptions, with an optional
translation, a chosen tone, and any extra experience the user wants to add.

**It ships with no API key.** Each visitor connects their own OpenAI or Anthropic key through
MasterKey, picks any model they like (they pay for it, so it's their call), and approves every
call on their phone after seeing the exact prompt and an estimated cost. The person who publishes
the app never holds anyone's key and never gets a token bill.
See [the MasterKey site](https://iamjoseph331.github.io/MasterKey/) for the phone side.

## Run it locally (mock vendor, no real credits)

```sh
# terminal 1: MasterKey with fake keys and a script that plays the phone
sdk/demo.sh --simulate-phone        # prints MK_APP_SECRET for "Resume Tailor"
# drop --simulate-phone to approve on the MasterKey iPhone app instead (dev login: demo@masterkey.dev)

# terminal 2
cd examples/resume-tailor && npm install
MK_URL=http://localhost:8080 MK_APP_SECRET=mk_app_... npm start    # http://localhost:3001
```

The mock vendor answers every call with a fixed sentence, so the output pane shows that instead
of a resume. Point `MK_URL` at a real MasterKey server (and approve with a real key) to get a
real resume.

| Env | Default | |
|---|---|---|
| `MK_URL` | `http://localhost:8080` | MasterKey server |
| `MK_APP_SECRET` | required | `curl -s -X POST $MK_URL/v1/apps -d '{"name":"Resume Tailor"}'` |
| `PORT` | `3001` | |
| `HOST` | `127.0.0.1` | set `0.0.0.0` to deploy |

## What the integration costs the developer

`server.js` uses the official SDKs. The only MasterKey-specific part is the base URL and using the
visitor's grant token where an API key would go:

```js
new OpenAI({ baseURL: MK_URL + '/v1/openai', apiKey: grant, maxRetries: 0 })
new Anthropic({ baseURL: MK_URL + '/v1/anthropic', apiKey: grant, maxRetries: 0 })
```

Plus the connect step (`POST /v1/connect`, then poll `GET /v1/connect/{code}` for the grant) and
polling `GET /v1/requests/{id}` when the visitor takes longer than 25 s to approve.
`maxRetries: 0` and an `Idempotency-Key` per job keep a slow or lost approval from ever being
billed twice.

## Notes

- Grants are kept per browser session in server memory (an HttpOnly cookie maps to them), so
  several visitors can use one deployment. A restart forgets them and visitors reconnect.
- Resume files are read in the browser (PDF via pdf.js, DOCX via mammoth, loaded from jsDelivr);
  only extracted text reaches the server. Scanned PDFs have no text, so users paste instead.
- The model list in `server.js` must match the MasterKey server's allowlist
  (`backend/src/pricing.rs`); prices shown are display only.
- Output is Markdown: copy it, download `.md`, or Print / PDF from the browser.
