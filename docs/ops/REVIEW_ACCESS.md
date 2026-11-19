# Reviewer access plan (App Review / TestFlight)

Goal: a reviewer exercises sign-in, add key, connect, request, allow/deny and account deletion with no paid provider key, no Joseph credentials and no public dev login.

## Plan
A separate Render service `masterkey-review` (free plan is fine; data resets, which suits fictional data), own `MK_MASTER_KEY`, own URL:
- `MK_VENDOR_MOCK=1` (fake vendor answers, "Hello from the MasterKey mock vendor."; accepts any well-shaped key such as `sk-proj-REVIEWFAKE0000`)
- `MK_PASSWORD_LOGIN=reviewer:<sha256hex(password)>` entered in the dashboard (random password from `openssl rand -base64 24`; give the password to Apple in App Store Connect review notes, never commit it)
- `MK_DEV_LOGIN=0`, no APNs, no Apple key. Mock mode also makes the OpenRouter button approve instantly.
- Deleting the reviewer account is allowed with the password (`POST /v1/me/delete {confirm:"DELETE", password}`); signing in again recreates it.
The app must offer this server (Developer options -> server URL, or a build-time constant for the review build).

Reviewer steps: sign in with the reviewer password -> Keys: paste `sk-proj-REVIEWFAKE0000` (or open `{server}/add-key` on a computer) -> create a connect code (a pairing page is needed: examples/resume-tailor or `POST /v1/apps` + `POST /v1/connect`, not in the backend paths) -> approve on the phone -> send a request -> Allow -> response shown.

## Built vs needs owner action
- Built and run locally 2026-11-18 against `MK_VENDOR_MOCK=1` + `MK_PASSWORD_LOGIN` (dev login 404): password sign-in, PUT fake key (`checked:true`), app + connect + approve, request + approve -> `completed`, text "Hello from the MasterKey mock vendor.", cost 0.000021 USD. Commands: start `MK_MASTER_KEY=$(openssl rand -base64 32) MK_VENDOR_MOCK=1 MK_PASSWORD_LOGIN="reviewer:$(printf %s "$PW" | shasum -a 256 | cut -d' ' -f1)" PORT=18771 backend/target/debug/masterkey-server`, then `POST /v1/auth/password`, `PUT /v1/me/keys/openai`, `POST /v1/apps`, `POST /v1/connect`, `POST /v1/me/connect/{code}/approve`, `GET /v1/connect/{code}`, proxy call, `POST /v1/me/requests/{id}/approve`.
- NOT done (owner): create the Render service, set the dashboard values, store the password for the review notes. Not verified: the iOS app against this server, a static reviewer pairing page, whether App Review accepts a mock backend.
