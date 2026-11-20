# Keyala rename: backend/ops cutover (MasterKey -> Keyala)

Status (2026-11-20): **source rename APPLIED** in `backend/`, `sdk/`, `examples/`, `render.yaml`, `docs/PLAN.md`, `docs/ops/` and my `docs/ux/`
files. Codex ran `cargo fmt`, `cargo test` (63 integration + unit) and `cargo clippy --all-targets`: green. The follow-up change of the connect-code
prefix `MK-` to `KY-` (generator, validator, tests, docs, SDK example, example CSS namespace `mk` -> `ky`, example icon/QR file names) was made by hand
edits afterwards and has NOT been re-run through cargo; run `cd backend && cargo test && cargo clippy --all-targets -- -D warnings` once more. The
one-off rename script is no longer needed (delete `.rename_keyala.py` if it is still in the repo root). Nothing here touches Render, GitHub or live data.
**Verification update (2026-11-20, per Codex):** after the `KY-` prefix change, source checks pass: 63 integration + 4 unit tests, `clippy --all-targets` green. Swift contract checks and an actual localhost Swift client (OpenRouter-only routing for OpenAI + Anthropic) passed; the full signed simulator build passed. The example QR was regenerated (CLI) and Vision decoded exactly `https://iamjoseph331.github.io/Keyala/` (this supersedes the "QR still encodes the old URL" note below). GUI checks passed: guide hide, server toggle save, ja language after restart. No real provider credits used. The repo is renamed to `iamjoseph331/Keyala`. Escrow/offsite copies of the master key and backups remain UNKNOWN.

**RELEASED (2026-11-20, Keyala 0.2.0):** prod main `5ad6fca0ef4e0f357e6824b54f547dca054e8545` (PR 16), deploy `dep-dbvvdcc9v7es73cc2sv0`, live 07:32:46Z; staging `36b5ef1` (PR 18), deploy `dep-dbvv9koae00c73ammatg`, live 07:24:11Z. Pages workflow 44694915696 and Backend CI 44694915708 succeeded. Both services verified live (details and the prod before/after data check in `PRODUCTION.md`): status 0.2.0 healthy with db true, dev login 404, preferences unauthenticated 401, `/sdk/keyala.js` 200, `/about` and `/privacy` say Keyala, AASA keeps the old Apple bundle id, public pairing gives `KY-` codes and `keyala_cs_` secrets, `seen` becomes `web`, links use `keyala://`, and the public Pages under `/Keyala/` return 200. Non-secret `KEYALA_*` env is applied on both existing service ids with the old `MK_*` vars kept for rollback. Not verified: TestFlight/phone install, live model credits, OpenRouter OAuth/billing, real Apple deletion, APNs, a real decrypt/paid response (only counts and integrity were checked).

**Actual live state (2026-11-20, after the release):**
- **Prod** `masterkey-prod` (srv-dbsm5m0ae00c738euic0): Starter + 1 GB disk, DB `/var/data/masterkey.db`, secret binding `MK_MASTER_KEY` unchanged (the code reads `KEYALA_MASTER_KEY`, else `MK_MASTER_KEY`; never regenerate). Non-secret `KEYALA_*` env is live; the old `MK_*` vars are retained for rollback.
- **Staging** `srv-dbsm5m0ae00c738euib0`: Free, no disk, not suspended, deploy `dep-dbvuikn40ujc73eu37ng`. Its disk `dsk-dbuielgm7kps73cgk420` and all backups/snapshots were permanently deleted after Joseph's explicit confirmation. The Staging section below is therefore already carried out in the Dashboard.
- Baseline cost: prod ~$7.25/month, staging $0, excluding extra usage and tax.

Not covered by this document: `ios/`, `site/`, `README.md`, `.github/`, `docs/launch/`, `docs/qa/`, `AGENTS.md` (Codex).

Example assets: `examples/resume-tailor/keyala-icon.svg` is the repo-owned koala copied from `site/keyala-koala.svg`. `keyala-qr.svg` was only renamed:
it encoded the OLD `/MasterKey/` Pages URL when renamed; it has since been regenerated and decoded as the `/Keyala/` URL (see the verification update above).

## Name map applied
| Old | New |
|---|---|
| MasterKey / Masterkey / masterkey | Keyala / Keyala / keyala |
| crate + binary `masterkey-server`, lib `masterkey_server` | `keyala-server`, `keyala_server` |
| env `MK_*` | `KEYALA_*` (except the exceptions below) |
| token prefixes `mk_app_`, `mk_sess_`, `mk_grant_`, `mk_cs_`, `mk_es_` | `keyala_app_`, `keyala_sess_`, `keyala_grant_`, `keyala_cs_`, `keyala_es_` |
| headers `X-MasterKey-*` | `X-Keyala-*` |
| errors `masterkey_expired`, `masterkey_uncertain` | `keyala_expired`, `keyala_uncertain` |
| URL scheme `masterkey://` | `keyala://` (`keyala://connect/{code}`, `keyala://enroll/{id}`, `keyala://oauth/openrouter`) |
| web SDK `/sdk/masterkey.js`, global `MasterKey`, element `<masterkey-connect>` | `/sdk/keyala.js`, `Keyala`, `<keyala-connect>` |
| GitHub Pages / repo `/MasterKey/` | `/Keyala/` (homepage in `KEYALA_PUBLIC_APPS` points at `https://iamjoseph331.github.io/Keyala/...`; Pages only serves that path after Codex renames the repo) |

## Intentional temporary exceptions (keep until the listed step is done)
1. **Apple bundle id and APNs topic `app.sorevan.masterkey`** (defaults in `lib.rs`/`apns.rs`, `render.yaml`, AASA `{team}.{bundle}`): the shipping app identity, Sign in with Apple audience and push certificate. Changing only the env would break Apple sign-in and universal links. Not renamed.
2. **`MK_MASTER_KEY`**: the code reads `KEYALA_MASTER_KEY` first, then falls back to `MK_MASTER_KEY` (temporary, one line in `Config::from_env`). `render.yaml` keeps declaring the secret under the OLD name so the Blueprint does not generate a new random key (that would make every stored provider key undecryptable). Codex copies the live secret to `KEYALA_MASTER_KEY` in the Dashboard (value never printed or regenerated), verifies the service boots, then removes the old key and the fallback, and renames the Blueprint key.
3. **Render service names `masterkey-prod` / `masterkey-staging`** and their onrender hosts (`masterkey-prod-vkgm...`, `masterkey-staging-vkgm...`): Blueprint matches services by name, so changing `name:` in the file first could create duplicate PAID services. Rename in the Dashboard first, observe the real new hostnames, then change `name:` and the host references. Existing service IDs stay the same. Hosts in docs/AASA/tests stay old until then.
4. **Prod database file `/var/data/masterkey.db`** and **backup file names `masterkey-<utc>.db`**: the live data. Renaming needs a verified file rename (stop writes or use the pause switch, take a `VACUUM INTO` backup, rename, restart, confirm counts) and a backup-prefix change in `ops.rs` (`backups()` filters and sorts by that prefix, so a half-renamed set would stop pruning). Do it as its own step, not with the rebrand. Local default and the staging DB are `keyala.db`.
5. **Design asset file names** `docs/ux/masterkey-icon*.svg|png`, `masterkey-appicon*` (old MK monogram exports referenced by iOS/QA docs): rename or replace together with the koala assets. The example's own icon and QR are already `keyala-*`.
6. **Connect code prefix** is now `KY-` (e.g. `KY-ABCD`, `connect_code_ok`). Codes issued before the change (`MK-...`) are no longer accepted; they only live 10 minutes. The monogram letters "MK" inside the web SDK dialog (`backend/src/keyala.js`) and `docs/ux` mock-ups are still the old icon: replace with the koala when the SDK gets artwork.

## Data and crypto compatibility (inspected)
- Encrypted provider keys: AES-GCM, AAD = `"{user_id}|{vendor}"` (`key_aad`). Enrollment candidates: AAD `"enroll|{id}"`. Sealed app secrets: AAD `"app-secret"`. **None contains the brand name; nothing changes.** Master key bytes unchanged, only the env var name.
- Token hashes in the database are SHA-256 of the full token. The prefix is only a format check, so **existing session, grant, connect-secret and enrollment tokens stop working** (they carry `mk_`): users sign in again, apps re-pair. Existing **app secrets** (sealed, `mk_app_...`) also stop matching the new prefix; developers must re-issue (Joseph accepted no backward compatibility). Stored provider keys, grants rows, requests and history are untouched.
- Migrations: `0001`..`0011` contain no brand names and are not edited (sqlx checksums unchanged); `0012_openrouter_routing.sql` is new.
- `PAUSE_UPSTREAM` file, `deleted-accounts.log`: unchanged names.

## Staging
`render.yaml`: `plan: free`, disk removed, `KEYALA_DATABASE_URL=sqlite:///data/keyala.db?mode=rwc`, no backup env. Joseph confirmed deleting the staging disk and downgrading (Codex operates the Dashboard). The Blueprint cannot delete a disk by itself; do it in the Dashboard first or the sync may complain. Staging data then resets on every deploy/sleep and staging-qa password login keeps working only if `KEYALA_PASSWORD_LOGIN` (renamed from `MK_PASSWORD_LOGIN`) is set in the Dashboard under its NEW name.

## Env variables that must be copied/renamed in the Dashboard (values not in git)
- Both services: `MK_MASTER_KEY` -> keep for now (exception 2).
- Staging: `MK_PASSWORD_LOGIN` -> `KEYALA_PASSWORD_LOGIN`; `MK_APNS_KEY_P8`/`MK_APNS_KEY_ID`/`MK_APNS_TEAM_ID` -> `KEYALA_APNS_*` if set; `MK_APPLE_SIGNIN_KEY_*` -> `KEYALA_APPLE_SIGNIN_KEY_*` if set.
- Prod: same `APNS`/`APPLE_SIGNIN` names if set. Non-secret values come from `render.yaml`.
- After the Blueprint syncs, any left-over `MK_*` variable is ignored by the new binary (except the master-key fallback).

## Cutover order (operational)
1. (Done, except the final cargo re-run noted above.) Codex does the iOS/site/CI side (scheme `keyala://`, associated domains unchanged until hosts change, repo rename).
2. Before deploying the new binary to a service, make sure its Dashboard env has the new `KEYALA_*` secret names (list above) so it does not boot without them. The binary refuses to start if the master key is missing (old stays live).
3. (Done 2026-11-20, except the phone step.) Staging first: disk deleted + downgraded, merged dev -> staging, `/healthz`, `/v1/status`, `/sdk/keyala.js`, `/about`, AASA checked. A phone sign-in and pairing with the NEW `keyala://` app build was not done.
4. (Done 2026-11-20.) Prod: backup `pre-keyala-20261120.db` taken first, database path unchanged, counts 1/2/0/0 before and after. Caution: that manual backup is outside the seven-copy automatic retention. The operator must retire it after the rollback window and apply deletions (`replay-deletions.sh`) to any restore made from it; no deadline is set. Rollback is NOT "redeploy the old image": migration `0012` is applied and an older SQLx binary may reject unknown migrations, so rollback needs a verified pre-migration DB recovery, deletion-log replay and preservation of any post-cutover data. Do not restore or overwrite the live DB otherwise.
5. Still to do, separately: Dashboard service rename + hostnames, `name:` and host edits, DB file rename with backup, master-key variable cleanup.

## CONTRACT CHANGE note for Codex to append to AGENTS.md
> 2026-11-20 [claude] CONTRACT CHANGE (Keyala rename, Joseph): all public names change at once, no backward compatibility: scheme `keyala://` (connect, enroll, oauth/openrouter), connect code prefix `KY-` (was `MK-`), token prefixes `keyala_app_|sess_|grant_|cs_|es_`, headers `X-Keyala-*` (e.g. `X-Keyala-Wait`, `X-Keyala-Request-Id`), error types `keyala_expired`/`keyala_uncertain`, env `KEYALA_*`, web SDK `/sdk/keyala.js` with global `Keyala` and `<keyala-connect>`, crate/binary `keyala-server`. UNCHANGED on purpose: Apple bundle id and APNs topic `app.sorevan.masterkey`, existing onrender hosts and service names, prod DB file `/var/data/masterkey.db`, backup file prefix, encryption format/AAD/master key (read from `KEYALA_MASTER_KEY`, else legacy `MK_MASTER_KEY`), migrations. Existing session/grant/app-secret tokens carry the old prefix and stop working; stored provider keys and history are intact. Staging: free plan, no disk, `/data/keyala.db`. Details and cutover order: docs/ops/KEYALA_CUTOVER.md.
