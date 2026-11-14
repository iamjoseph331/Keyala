# AGENTS.md — shared rules for Claude Code and Codex

Two agents (Claude Code, Codex) work on this repo for one human owner. This file is the
coordination channel. Codex reads it automatically; Claude Code imports it via `CLAUDE.md`.

## Protocol
1. **Read `docs/PLAN.md` and the log below before starting any work.** `git pull --rebase` first.
2. **Ownership** — only edit paths you own. Need a change elsewhere? Append a request to the log.
3. **The API contract (`docs/PLAN.md` § Contract) is the source of truth.** Change it only by
   appending a `CONTRACT CHANGE` entry to the log *and* editing the contract in the same commit.
4. **Small commits, push often**, prefix with area: `backend:`, `ios:`, `site:`, `docs:`.
   Never force-push `main`. Never rewrite another agent's commits.
5. **Resources are limited.** Plan with the big model, implement with smaller/cheaper models
   (Claude: Sonnet/Haiku subagents; Codex: mini models). No speculative features.
6. **Decisions you both can't settle → ask the owner.** Log the question as `QUESTION:`.
7. Secrets never go in git. Real API keys never appear in tests, fixtures, logs or screenshots.

## Ownership (proposed by Claude — Codex: reply in the log with ACK or counter-proposal)
| Path | Owner |
|---|---|
| `backend/` (Rust server, DB, migrations) | Claude Code |
| `sdk/` (TS + Python thin clients, examples) | Claude Code |
| `ios/` (SwiftUI app) | Codex |
| `site/` + `README.md` (GitHub Pages, demo media) | Codex |
| `docs/PLAN.md`, `AGENTS.md` | shared (contract-change rule above) |

Rationale: the contract is the seam. Backend and iOS can be built in parallel against it;
the iOS side can mock the server from the contract until the backend runs.

## Log (append-only, newest at bottom: `- YYYY-MM-DD [agent] message`)
- 2026-11-14 [claude] Created AGENTS.md + docs/PLAN.md (architecture, contract, milestones). Awaiting Codex ACK on ownership and owner answers on open questions (PLAN.md § Open questions).
