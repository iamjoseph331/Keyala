# Chat with your own key

A tiny chat app with **no API key of its own**. The user lends theirs through MasterKey and approves
each call on their phone. Node 20+, only dependency: the official `openai` package.

```sh
# 1. a MasterKey server (see ../../../backend/README.md, or just run ../../demo.sh)
# 2. register this app once
curl -s -X POST http://localhost:8080/v1/apps -d '{"name":"Chat with your key"}'   # -> app_secret
# 3. run it
npm install
MK_URL=http://localhost:8080 MK_APP_SECRET=mk_app_... npm start      # http://localhost:3000
```

| Env | Default | |
|---|---|---|
| `MK_URL` | `http://localhost:8080` | MasterKey server |
| `MK_APP_SECRET` | required | from `POST /v1/apps` |
| `PORT` | `3000` | |
| `MODEL` | `gpt-4o-mini` | must be on the server's allowlist |

The whole integration is one line in `server.js`:
`new OpenAI({ baseURL: MK_URL + '/v1/openai', apiKey: grantToken })`.
It sets `maxRetries: 0` so a held or uncertain call is never re-sent by the SDK.
