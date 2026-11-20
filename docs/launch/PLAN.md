# Keyala launch readiness

Owner decision 2026-11-18: free, invite-only TestFlight first; English, Traditional Chinese (zh-Hant), Japanese. Public App Store release follows beta evidence. Operations contact: josephchen637@gmail.com. Joseph handles OpenAI/Anthropic/OpenRouter top-ups. Resume Tailor rework is separate, owned by Joseph + Claude.

## Execution order and acceptance

| Gate | Owner | Acceptance | State |
|---|---|---|---|
| Support/contact | Codex | Native support + privacy information and three-language website pages use the authorized email; no secrets in reports | implemented in source; local checks pass, visual/deployment checks pending |
| Three native languages | Codex | English/zh-Hant/ja, runtime errors and permissions; signed build and native visual checks | implemented in source; local checks pass, visual/deployment checks pending |
| Account deletion | Claude backend + Codex UI | Fresh identity confirmation, owner isolation, all associated data/access removed, Apple revocation handled; sending/uncertain cases safe | backend + native implemented; local lifecycle tests pass, deployment/live Apple checks pending |
| Lost-device controls | Claude backend + Codex UI | Revoke all sessions / stop app permissions, no automatic retry or new upstream sends after stop | backend + native implemented; local lifecycle tests pass, deployment/live Apple checks pending |
| Durable production | Claude | Reviewable config, cost, restore rehearsal, restart persistence, rollback; paid change and main promotion await final approval | prepare only |
| Operations | Claude | Consistent SQLite backup, separate master-key handling, mock restore drill, health/incident/stop-forwarding runbook | backend implemented; local restore rehearsal recorded, hosted rollout pending |
| Real provider success | Joseph + agents | One small request per provider, phone Allow, response, usage/cost evidence; quota/uncertain/retry checks | awaiting top-ups |
| Device distribution | Codex + Joseph | TestFlight signing, production APNs, lock-screen privacy, notification tap, biometrics and language UI | remaining |
| Reviewer route | Claude + Codex | Reviewers exercise full flow without their own paid key; isolated restricted review data/service, no public dev login | prepare |
| Privacy/store metadata | Codex + Joseph | Data handling matches actual deployment, support/privacy URLs live, beta review notes + screenshots; no unresolved placeholders | preparing |
| Private pilot | Joseph | 3 developer partners + 10 invited iPhone testers; consent-based feedback, first-success evidence | planned |

## Execution boundaries

Push only dev. Staging changes follow existing PR flow after tests and contract ACK. Prepare public copy and outreach drafts, but do not post, email others, invite testers, submit to Apple, promote main/Pages or buy resources without the applicable final owner action/approval. A public email address is a contact setting, not permission to send mail or create a Gmail account.

Existing uncommitted Xcode signing/workspace edits belong to Joseph and stay unstaged. Do not change the API contract outside the shared CONTRACT CHANGE protocol. Keep enrollment candidates immutable. Never infer successful billing from a quota-error response, or invoice USD from token-derived usage cost.

## Privacy decisions

No new automatic history expiration is introduced: the original product promises later inspection. Define deletion and backup retention with the implemented backend; do not publish a deletion deadline unsupported by the restore process. Hosted Keyala handles keys and request contents; encryption at rest does not hide them from its operator. No advertising tracker or third-party analytics is added for this launch.

## Sources checked 2026-11-18

- https://developer.apple.com/support/offering-account-deletion-in-your-app/
- https://developer.apple.com/help/app-store-connect/test-a-beta-version/invite-external-testers
- https://developer.apple.com/app-store/review/guidelines/
- https://news.ycombinator.com/showhn.html
