# AGENTS.md — shared rules for Claude Code and Codex

Claude Code and Codex share this repository. Codex reads this file automatically;
Claude Code imports it through `CLAUDE.md`. Keep public collaboration rules here.
Personal notes, deployment details and agent history belong only in the ignored
local log at `.local/coordination/LOG.md`.

## Protocol

1. **Read `docs/PLAN.md` and the local coordination log before starting work.**
   Run `git pull --rebase` first. If existing uncommitted changes prevent it,
   preserve them and fetch to inspect incoming changes; do not stash or discard
   another person's work. A fresh checkout may have no local log; public docs
   must remain sufficient to understand the implementation.
2. **Ownership:** only edit paths you own. Record requests for changes elsewhere
   in the local log. Check it for active handoffs before working on shared paths.
3. **API contract:** `docs/PLAN.md` is the source of truth. Record a
   `CONTRACT CHANGE` in the local log and update the public contract in the same
   commit as its implementation. Never require private history to understand a
   public API change.
4. **Branches:** work and push on `dev` (CI only). Promote by PR `dev` ->
   `staging` -> `main`. Staging deploys the test service; main deploys production
   and publishes GitHub Pages. Never push directly or force-push to `main`.
5. **Small commits, push often.** Prefix commits with `backend:`, `ios:`, `site:`
   or `docs:`. Stage only your own changes. Never rewrite another agent's commits.
6. **Resources are limited.** Plan with the large model and use smaller models
   for bounded implementation work when available. No speculative features.
7. **Unsettled decisions:** ask the owner and append a `QUESTION:` entry to the
   local log.
8. **Secrets and private records never go in Git.** Real API keys must not appear
   in tests, fixtures, logs or screenshots. Never commit `.local/`, internal
   coordination history, personal contact details or deployment-specific notes.

## Default ownership

| Path | Owner |
|---|---|
| `backend/` (Rust server, DB, migrations) | Claude Code |
| `sdk/` (TS + Python clients and examples) | Claude Code |
| `ios/` (SwiftUI app) | Codex |
| `site/` + `README.md` (GitHub Pages and demo media) | Codex |
| `docs/PLAN.md`, `AGENTS.md`, `roadmaps.md` | Shared |

Task-specific handoffs and temporary ownership changes go in the local log.
The public API contract is the seam between parallel backend and client work.

## Local coordination

Append handoffs, decisions and validation records to `.local/coordination/LOG.md`
using `- YYYY-MM-DD [agent] message`, newest at the bottom. Keep existing entries
intact. The complete earlier `AGENTS.md` snapshot is retained locally as
`.local/coordination/AGENTS-history-20261122.md`; do not add it to Git.
