# Keyala rename inventory and execution

Joseph confirmed Keyala and the koala mascot on 2026-11-20, after Codex listed
these areas BEFORE the first rename operation. No backward compatibility is
required, but production data and encryption identity must remain readable.

| Area | Scope |
|---|---|
| GitHub | Repo name, origin, badges, Actions, Pages path, links |
| Website | Brand, metadata, social image, tutorial/privacy/support copy, koala SVG |
| iOS | Display name, project/target/source paths, strings, icon and brand mark |
| Apple | Bundle ID, Apple login audience, APNs, AASA, signing identity |
| Links | URL scheme, connect/enroll/OAuth QR and callback, associated hosts |
| Server/SDK | Crate/binary, env, tokens, errors, headers, Web SDK and examples |
| Operations | Render labels/URLs, storage and backup naming, secrets, runbooks |
| Coordination | Current docs, Claude handoff, local checkout/workspace |

## Selected mapping

- Repository: `iamjoseph331/MasterKey` -> `iamjoseph331/Keyala`.
- Website: `https://iamjoseph331.github.io/Keyala/`.
- Native project: `ios/Keyala/Keyala.xcodeproj`, target `Keyala`.
- URL scheme: `keyala://`; SDK global: `Keyala`; component: `keyala-connect`.
- Server package/binary: `keyala-server`; env: `KEYALA_*`; token prefix: `keyala_`.
- API headers: `X-Keyala-*`; see updated contract in `docs/PLAN.md`.
- Koala art is a repository-owned SVG at `site/keyala-koala.svg`, rendered for
  the native icon; it is not a third-party trademark asset.

## Deliberate remaining external identifiers

The existing Apple bundle `app.sorevan.masterkey` is preserved so the shipping
identity, login audience, signing and APNs remain aligned. Changing it requires
verifying App Store Connect records and provisioning, not a blind text edit.

Existing Render service IDs, hostnames and Blueprint resource names stay in
place pending a coordinated infrastructure rename. Do not create replacement
paid services. Production `/var/data/masterkey.db`, its backup prefix, and the
legacy `MK_MASTER_KEY` secret binding remain until a verified data/secret migration.
The new server reads that master-key value without regenerating it.

The physical checkout stays at `/Users/macbook/Code/Sideprojects/Masterkey` while
Codex and Claude use it; moving an active workspace would invalidate both sessions.
Historical append-only logs, research snapshots and old screenshots retain their
original names as evidence. They are not the current product identity.

## Staging allocation

Joseph confirmed irreversible deletion of the staging disk, including same-disk
backups and snapshots. Render now reports Free compute and no disk. It resets
state on deploy/restart/sleep. Prod retains Starter plus its 1 GB disk, approximately
US$7.25/month baseline excluding additional usage and taxes.

## Verification and cutover

Update this section with actual commit/deploy evidence; a source rename alone is
not evidence that the live backend, Pages or a distributed iPhone build changed.
Backend/crypto constraints and the operational sequence are in
`docs/ops/KEYALA_CUTOVER.md`. Claude authored the backend rename script; Codex ran
it and verified the Rust tests and clippy, then removed the one-off script.
