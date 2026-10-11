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
for the combined commit.

That dispatched run passed, but its check suite was not associated with the
queue commit's GraphQL status rollup, so it did not complete the queue gate.
The durable correction merged in [PR #13](https://github.com/closure-labs/mish/pull/13):
repository-scoped GitHub App authentication and native `merge_group` events.
App registration/installation and the documented Actions variable/secret are
an outstanding configuration dependency; no personal token is stored in CI.

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
- [x] Native rebrand checks passed at `ec0dd6a`; the complete rebrand is
  consolidated in Rust [PR #14](https://github.com/closure-labs/mish/pull/14).
  Superseded PR #10 is closed; delivery follows the current Rust gate.

## Rust-native coordinator

- [x] User-approved scope and Codex Overkill comparison:
  `docs/rust-architecture.md`; one coordinator remains authoritative.
- [x] Implementation: validated domain types, bounded typed command mailbox,
  dedicated SQLite thread, Axum/Clap adapters and native USB serial framing.
  The appliance package contains no Python runtime.
- [x] Preserve commissioning, staff tokens, alerts/ACKs, private/public
  projections, legacy opt-in content commands and snapshot no-replay recovery.
- [x] Independent security review: no P0/P1 blockers. Corrected browser request
  payloads, SIGTERM lifecycle and journal polling N+1 queries.
- [x] Cargo behavioral tests and strict Clippy pass; all-system Nix evaluation
  passes. Final realized package/VM and GitHub checks follow the frozen revision.
- [x] Local native package/VM: 38 Rust tests, strict formatting/Clippy,
  CLI/schema/workflow/module checks and booted HTTPS/restart/recovery scenario
  passed for application source `c04f529`. Runtime closure contains no Python.
- [x] Native x86 and ARM GitHub checks passed on `1d7d68d` in
  [run 38177392268](https://github.com/closure-labs/mish/actions/runs/38177392268).
  Final evidence-only revision repeats both required checks.
- [x] Committed and pushed Rust implementation `a6e4fb8` with follow-up
  coordinate parsing and current-main merges; [PR #14](https://github.com/closure-labs/mish/pull/14).
- [x] Successful native checks and merge-queue delivery of final Rust revision:
  PR #14 merged as b0bc113 after both native merge-group checks passed.

Physical hardware and field-pilot acceptance remain open.

## Shared binary cache and validation outputs

- [x] User-approved scope: use CACHIX_PUBKEY and CACHIX_TOKEN to share
  Mish, other build outputs, native check evidence and the developer shell.
- [x] Architecture: one signed public cache, trusted main/tag uploads,
  token-free PR/merge-group reads, explicit roots plus newly built outputs.
  Use identical local/CI Nix checks for validation reuse through nix develop.
- [x] Runtime audit: all existing JavaScript action pins use Node 24;
  new Cachix v17 uses Node 24. Full SHA pinning remains in effect.
- [x] Independent security review approved after complete evidence checksums;
  12 CI-helper tests pass, including read-only and failed-validation no-upload
  regressions. All JavaScript action runtimes were verified from pinned metadata.
- [x] Workstation resource constraint: stopped overlapping evaluations/builds.
  Added one local lock, one Nix job/core, and a 2 GiB user scope that includes
  shell startup. Full native/VM/image validation runs on GitHub runners.
- [x] GitHub native checks and queue delivery: PR #16 merged as `e6cd187`.
- [x] Main upload and signed-cache reuse: Checks run 38181091031 uploaded
  outputs; cold-cache run 38181529187 passed on both native runners with GitHub
  store restores disabled. Logs show package, shell and check outputs copied
  from `https://mish.cachix.org` without upload credentials.
  Images run 38181091160 built both architectures and uploaded image/cache
  closures successfully.

## CI qualification gates and dependency graph

- [x] Scope and architecture: full native checks for every PR/merge group;
  descriptive change qualifiers; explicit software/image gates; physical gates
  remain open. Great CTO infrastructure specialist reviewed the gate design.
- [x] Implementation: machine-readable dependency policy, actual-workflow
  validation, fail-closed prerequisite guards and qualification evidence.
- [x] Independent code/security review approved; 20 lightweight helper tests
  pass under the bounded nix develop shell, including gate and artifact failures.
  Qualification tests also pass against read-only source fixtures. Metadata and
  maintenance-update jobs do not claim native validation snapshot keys.
- [x] Active merge rules and queue admission require Software qualification
  alongside both unchanged native contexts. Prior run 38182728905 passed both
  native jobs and aggregate qualification; final snapshot changes repeat CI.
- [x] Native GitHub checks and merge-group delivery: PR #17 merged as
  `3061197`; final PR checks run 38183355942 passed both native contexts and
  Software qualification.

## Community, staff and department radio enrollment

- [x] User scope: existing channel join or regional mesh creation, independent
  256-bit keys, stock Meshtastic enrollment QR codes; public radio participation
  replaces the read-only attendee model. Default browser role is enrollment.
- [x] Architecture review: stable channel identities bind queued records;
  SQLite stores credentials and membership; native USB commissioning verifies
  readback, and reconnect never rewrites channels. Keys stay out of Nix/GitHub.
- [x] Rust channel codec, generation, QR and protected native provisioning.
- [x] Scoped registry, application/CLI enrollment and channel-aware outbox.
- [x] Independent security review approved after private default-selection and
  scoped journal fixes. Local behavioral checks: 68 Rust tests, seven Node 24
  browser-selection tests, strict Clippy and formatting passed. Four native
  USB fixtures exercise verified writes, wrong readback, disconnect and RF drift.
- [ ] Independent security/QA and native GitHub qualification.
- [ ] Merge queue delivery. Physical app/USB qualification remains open.
