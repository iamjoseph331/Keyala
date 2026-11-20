# keyala.io integration plan

Status (2026-11-20): Phase 0 DONE and verified (HTTPS on all four hosts, www + old github.io path 301 to keyala.io, AASA on app.keyala.io, support@keyala.io via Email Routing). Phase 1 in source on `dev`, not deployed. Phases 2-3 handed to Codex.

## Hostnames

| Host | Serves | Backed by |
|---|---|---|
| `keyala.io` (+ `www` redirect) | homepage, docs, user guide, privacy, support, examples | GitHub Pages (from `main`) |
| `app.keyala.io` | API, `/connect`, `/add-key`, `/enroll`, AASA, `/sdk/keyala.js`, future web console | Render `masterkey-prod` |
| `staging.keyala.io` | same, staging | Render `masterkey-staging` |

The `*.onrender.com` hosts keep working forever (Render can't turn them off), so old QR codes, SDK snippets and app builds don't break. Render service names and IDs stay as they are.

## Phase 0: Joseph, in the dashboards (~30 min, all reversible)

1. **GitHub, verify the domain first** (prevents takeover): Settings > Pages > Verified domains > add `keyala.io`, add the TXT record it shows in Cloudflare DNS.
2. **Cloudflare DNS** (every record **DNS only / grey cloud**, so GitHub and Render can issue their own certificates and Render keeps its own Cloudflare edge, which gives us `CF-IPCountry`):
   - `keyala.io` CNAME `iamjoseph331.github.io` (Cloudflare flattens it at the apex)
   - `www` CNAME `iamjoseph331.github.io`
   - `app` CNAME `masterkey-prod-vkgm.onrender.com`
   - `staging` CNAME `masterkey-staging-vkgm.onrender.com`
3. **GitHub repo** Settings > Pages > Custom domain `keyala.io`, wait for the certificate, then Enforce HTTPS. `iamjoseph331.github.io/Keyala/...` then redirects to `keyala.io/...` (that also fixes the 404 from the old `/MasterKey/` links that are already out there).
4. **Render**: on each service, Settings > Custom Domains > add `app.keyala.io` / `staging.keyala.io`, wait for "Certificate issued".
5. Optional: **Cloudflare Email Routing** `support@keyala.io` -> your Gmail (free), so the public contact address is no longer your personal one.

Check: `curl -sI https://keyala.io`, `curl -s https://app.keyala.io/healthz`, `curl -s https://app.keyala.io/.well-known/apple-app-site-association`.

## Phase 1: backend + examples (Claude), staging first

- `render.yaml`: `KEYALA_PUBLIC_URL` = `https://staging.keyala.io` / `https://app.keyala.io`. This one value moves `connect_url`, QR codes, enroll links and the OpenRouter OAuth callback (`backend/src/oauth.rs`). Promote only once that service's custom domain shows a certificate.
- `KEYALA_PUBLIC_APPS`: Resume Tailor homepage -> `https://keyala.io/examples/resume-tailor/`, `allowed_origins` gets `https://keyala.io` (keep the github.io origin until Pages redirects are confirmed).
- `backend/src/routes.rs` `GET_APP_URL` and `backend/src/keyala.js` (`GET_APP`, default `SERVER`, header comment) -> `https://keyala.io/` and `https://app.keyala.io`; `sdk/web/README.md` snippet.
- `examples/resume-tailor`: `SERVERS` map, "Get Keyala" links, regenerate `keyala-qr.svg` for `https://keyala.io/`.
- Verify on staging: QR decodes to the new host, a phone approval still lands a country in `GET /v1/app/report` (the `CF-IPCountry` check must be repeated on the custom domain), OpenRouter one tap completes.

## Phase 2: iOS (Codex)

- `Keyala.entitlements`: add `applinks:app.keyala.io` and `applinks:staging.keyala.io`; keep the two onrender.com entries for at least one release.
- `Models.swift` `productionURL` / `stagingURL` -> new hosts.
- `AppStore.swift` `supportsStagingPassword` compares against the hard-coded onrender staging URL; must accept `staging.keyala.io` too.
- `ContractTests.swift` OAuth callback fixture.
- Order: an app build with the new applinks should be on the phone before Phase 1 reaches prod. If it isn't, scanning a new QR opens the `/connect` web page instead of the app, which still has the "Open in the Keyala app" (`keyala://`) button, so it degrades, it doesn't break.

## Phase 3: site + README (Codex)

- All `iamjoseph331.github.io/Keyala/...` links -> `https://keyala.io/...` (README, `og:image`, examples README).
- Developer console: default API `https://app.keyala.io`, list both old and new hosts as allowed choices.
- Tutorials and snippets: `base_url` `https://app.keyala.io/v1/openai` etc.

## Later (not part of this cutover)

- Web console on `app.keyala.io`: Sign in with Apple on the web needs a Services ID with return URL `https://app.keyala.io/...` (Joseph, Apple Developer portal).
- App Store listing: `GET_APP_URL` -> listing URL.

## Rollback

Each phase can be undone on its own: remove the custom domain or DNS record, revert `KEYALA_PUBLIC_URL` (empty = `RENDER_EXTERNAL_URL`). No data or secrets move; the master key and database don't change.
