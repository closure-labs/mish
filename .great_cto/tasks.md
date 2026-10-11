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
- [x] GitHub QA: native x86 and ARM Checks passed for `c91498b` in
  [run 38172407182](https://github.com/closure-labs/mish/actions/runs/38172407182).
- [x] Delivery: committed, pushed and opened
  [PR #7](https://github.com/closure-labs/mish/pull/7); merge-queue validation
  follows the repository's required checks.

## Automatic PR queue admission

- [x] Inspect effective rules: API reported no branch protection, rulesets or
  merge queue for main. Applied the versioned `.github/merge-queue-rules.json`
  as active ruleset `24893052`, requiring both native Checks and ALLGREEN queue
  validation with one concurrent group and no bypass actors.
- [x] Implementation: trusted-main workflow and expected-head queue API;
  same-repository branches only; failed, missing, stale, draft, conflicting or
  review-blocked PRs wait. Dispatch queue CI only when its run is absent.
- [x] Independent security review: approved after adding fork rejection.
- [x] Local QA: five queue-policy tests; full suite now totals 43 tests.
- [x] GitHub QA: both native jobs passed for `67c3347` in
  [run 38173427998](https://github.com/closure-labs/mish/actions/runs/38173427998).
  Both merge-group jobs passed in
  [run 38173802058](https://github.com/closure-labs/mish/actions/runs/38173802058);
  [PR #8](https://github.com/closure-labs/mish/pull/8) merged as `fb6db84`.

End-to-end automatic admission was exercised by
[PR #9](https://github.com/closure-labs/mish/pull/9), opened without manual
queue enrollment. [Merge queue run 38174507668](https://github.com/closure-labs/mish/actions/runs/38174507668)
admitted its validated head and explicitly dispatched
[queue Checks run 38174603025](https://github.com/closure-labs/mish/actions/runs/38174603025)
for the combined commit. PR #9 subsequently merged as `503ed429`; the
rebrand retains that automatic-admission evidence when updating its main base.

Physical M2–M5 gates remain open. This task does not publish or deploy a release.

## Mish rebrand

- [x] User-approved scope: use Mish internally and externally before the first
  deployment. Rename CLI, Python package, Nix/module/image identifiers,
  service users, runtime paths, environment variables, catalog and schemas.
- [x] Implementation: one consistent namespace; commissioning, recovery,
  workflow targets and regressions follow the new interfaces.
- [x] Documentation: benefit-led README and operator/developer entry points;
  current capabilities and future concepts remain grounded in evidence.
- [x] Independent review: infrastructure interface consistency and docs claims.
- [x] Local QA: all 43 Python tests, all-system evaluation and native x86 checks
  passed, including the booted Mish VM and snapshot-recovery scenario.
- [ ] GitHub QA and automatic queue delivery of the reviewed revision.
