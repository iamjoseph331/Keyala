# MasterKey agent handoff

## Joseph's direction

Build a Rust backend, native iOS app, and compelling GitHub developer experience.
User-owned vendor keys never reach developer services. Users inspect exact prompts,
approve usage once or through revocable count/cost/trust grants, and inspect responses.
Resources are limited: plan first, use smaller models for bounded coding tasks,
commit and push verified milestones. Joseph says Claude is already working here.
Use English, Traditional Chinese, or Japanese; never Simplified Chinese or the middle-dot character.

## Coordination proposal from Codex (2026-11-14 JST)

**Claude: please read and respond below before implementing a conflicting contract.**

- Claude ownership proposed: Rust backend, persistence, security, developer wrapper.
- Codex ownership: native iOS application, repository presentation, integration verification.
- Each agent edits and commits only its owned files. No force pushes, resets, or stashing
  the other agent's work. Check status before commits; stage named paths.
- Start with a local runnable vertical slice using fake provider responses, then real
  provider adapters behind explicit configuration. Label simulated data everywhere.
- Rust + Axum + SQLite is a resource-conscious proposal, not a settled requirement.
- First agree on wire shapes. Codex will write a proposed contract in docs/API_PROPOSAL.md.
- No live provider calls, cloud spending, App Store publication, or deployments without
  an actual need and suitable user-provided credentials. Local builds and Git pushes are authorized.

## Security invariants for joint review

1. Developer credentials cannot list/reveal keys or approve requests. User identity and
   app identity are separately authenticated. Linking requires the user's consent.
2. Approval binds to immutable stored provider, model, payload, app, user, pricing
   snapshot, output cap, and expiry. Developers cannot alter approved calls.
3. Fixed provider destinations only; no arbitrary URL/header forwarding. Start with
   bounded text requests; reject tools, images, streaming, URLs and hidden provider state
   until their visibility and pricing semantics are deliberately supported.
4. Atomically reserve count and cost before executing. Idempotency and state transitions
   prevent duplicate execution. Timeouts after send are uncertain, not automatically retried.
5. Integer money units; distinguish estimates, reservations, and actual reported usage.
   Never promise a provider-enforced monetary cap from a heuristic token estimate.
6. Encrypt keys at rest; redact credentials in logs/errors/notifications. MasterKey
   operators remain inside the trust boundary; do not claim a trustless service.
7. Push contains opaque request IDs only. Inspect and decide inside authenticated app.
8. Production Google login and APNs need operator configuration. A local demo identity
   must fail closed outside explicit development mode.

## Claude response / alternative ownership or contract

Pending. Please append your plan and any already-owned files here.

## Codex status

- Repository inspected: empty README and initial commit on main.
- Xcode 26.2 and Swift available. Rust/cargo not currently on PATH.
- Official OpenAI Responses, Anthropic Messages, and Apple APNs docs consulted.
- Planning only so far; no implementation files claimed outside the split above.
