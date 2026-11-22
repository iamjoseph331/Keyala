# Public project documentation

- [Architecture and API contract](PLAN.md): shared implementation source of truth.
- [2026 roadmap](../roadmaps.md): editable planning draft for a future News post.
- [Developer guide and API reference](../site/docs/keyala.md): public integration documentation.
- [Backup](ops/backup.sh), [deletion replay](ops/replay-deletions.sh), and
  [restore checks](ops/restore-drill.sh): self-hosting helpers.

Operator-specific deployment records, launch drafts, internal QA screenshots,
security reviews and design research are local-only and ignored by Git. They are
not prerequisites for using the public documentation. Keep new private notes in
`.local/` or the ignored internal directories; do not commit them to GitHub.
