# Development phase record

Beads is unavailable in this environment; this file records phase tasks.

## Snapshot recovery — M3 software acceptance

- [x] Project audit: reproduce queued snapshot replay after the original
  coordinator transmitted the message. Read-only specialist audit reproduced
  the fault through `nix develop` on baseline `fabce7e`.
- [x] Architecture: retain the approved single coordinator and SQLite model.
  Restore to fresh state, refuse overwrites, validate a standalone backup,
  quarantine snapshot queue entries and retain restore provenance.
- [x] Implementation: restore CLI and eight rejection/durability regressions.
- [x] Review: independent security and QA approved after fixing unsupported
  extra/reordered columns. Reference runtime schema and uniqueness checks are
  shared with restore validation.
- [x] Local QA: 38 Python tests, all-system evaluation, native x86 checks and
  booted VM snapshot recovery passed. Workflow actionlint/shellcheck passed.
- [ ] GitHub QA: native x86 and ARM results for the PR revision.
- [ ] Delivery: commit, push and reviewable GitHub PR with validation evidence.

Physical M2–M5 gates remain open. This task does not publish or deploy a release.
