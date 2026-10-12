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
  replaces the read-only attendee model. Confirmed community participation uses
  the Meshtastic app only; the public browser provides QR enrollment.
- [x] Architecture review: stable channel identities bind queued records;
  SQLite stores credentials and membership; native USB commissioning verifies
  readback, and reconnect never rewrites channels. Keys stay out of Nix/GitHub.
- [x] Rust channel codec, generation, QR and protected native provisioning.
- [x] Scoped registry, application/CLI enrollment and channel-aware outbox.
- [x] Independent security review approved after private default-selection and
  scoped journal fixes. Local behavioral checks: 68 Rust tests, seven Node 24
  browser-selection tests, strict Clippy and formatting passed. Four native
  USB fixtures exercise verified writes, wrong readback, disconnect and RF drift.
- [x] Independent security/QA and native GitHub qualification: both native
  architectures, the booted x86 VM and Software qualification passed for
  `b9ffef5` in [run 38185483836](https://github.com/closure-labs/mish/actions/runs/38185483836).
- [x] Merge queue delivery: all three required contexts passed in
  [run 38186040176](https://github.com/closure-labs/mish/actions/runs/38186040176);
  [PR #18](https://github.com/closure-labs/mish/pull/18) merged as `b4dec1c`.
  Physical app/USB qualification remains open. Automatic queue admission still
  awaits the documented GitHub App credentials; CLI admission exercised the
  native queue without bypassing checks.

## Den aspect composition and product review

- [x] User scope: reshape the flake using the local Den reference; organize
  modes, features and architectures, and review roadmap/progress/status.
- [x] Great CTO infrastructure architecture review: real Den host generation
  and aspect composition, supported by the minimal template and output guide.
  Pin the reference's upstream revision; preserve existing public outputs and
  Pi 5 vendor instantiation. Scope is the Nix composition subsystem.
- [x] Independent product audit: M0/M1 software gates passed; M2–M5 physical
  acceptance remains open. Current main `a4660ac` passed native Checks
  [38187467343](https://github.com/closure-labs/mish/actions/runs/38187467343)
  and Images [38187467354](https://github.com/closure-labs/mish/actions/runs/38187467354).
  App credentials remain absent; this is a separate repository dependency.
- [x] Product report and stale status corrections: `docs/product-status.md`,
  roadmap, verification and commissioning; hardware qualification precedes
  event-kit acceptance, ARM qualification and rehearsal.
- [x] Sequential implementation: Den dependency, aspect layers, stable host
  generation and output/check adapters.
- [x] Independent code/QA/security review approved. Fixed reusable state-version
  defaults and routed the VM through the actual Den simulation mode. Native
  composition tests cover valid modes and specific device/mode/architecture
  rejection assertions. Bounded local package/export inventories, one x86
  host and one simulation/state-version projection passed.
- [x] Native software, VM, image and merge-group qualification: candidate
  `8b90573` passed Checks
  [38189634053](https://github.com/closure-labs/mish/actions/runs/38189634053)
  and Images [38189631186](https://github.com/closure-labs/mish/actions/runs/38189631186).
  All three queue contexts passed in
  [38190366390](https://github.com/closure-labs/mish/actions/runs/38190366390).
  Initial GitHub qualification caught an omitted platform in the standalone
  wrong-architecture test fixture. The fixture now supplies the platform that
  Den supplies on real hosts; a bounded rejection projection passed. Production
  assertions and fail-closed workflow gates remain intact.
- [x] Commit, push and merge queue delivery:
  [PR #20](https://github.com/closure-labs/mish/pull/20) merged as `8381b5f`.
  Physical deployment remains open. Status/evidence closeout changes are
  documentation only and follow the same required checks.

## Initial alpha release preparation

- [x] User scope: use the changelog-generator skill and prepare the first alpha.
  Preserve `0.1.0-alpha` across Cargo and Nix; publication and physical deployment
  remain outside this draft preparation.
- [x] Changelog specialist: summarize delivered features, prototype interface
  changes, recovery fixes and physical limitations from the Git history.
- [x] Infrastructure review: identify missing prerelease/curated notes/version
  guards and interrupted-upload handling in the existing release helper.
- [x] Implementation: matching tag/version/notes, exact source, both lockfiles,
  actual image payload/target checks, isolated upload staging, aggregate checksums
  and same-source draft-only reruns. No publication operation is introduced.
- [x] Bounded local QA: 27 repository-helper tests passed, including release
  rejection, checksum coverage and interrupted-draft behavior. Full native
  checks, workflow lint, VM and image builds run in GitHub.
- [x] Independent infrastructure, security and QA release review approved.
  Added upload-failure coverage, explicit draft retention and final identity
  verification; seven release-helper tests passed after the final adjustment.
- [x] Commit, PR and native merge-group qualification: [PR #22](https://github.com/closure-labs/mish/pull/22)
  merged as `f3921c9` after native [Checks 38192810349](https://github.com/closure-labs/mish/actions/runs/38192810349)
  and [merge-group 38193246186](https://github.com/closure-labs/mish/actions/runs/38193246186) passed.
- [x] Annotated `v0.1.0-alpha` tag points to that exact revision. Tagged
  [Draft release run 38193676481](https://github.com/closure-labs/mish/actions/runs/38193676481)
  passed native checks, x86 VM and both image builds, then created a draft
  prerelease. Verified 39 manifest-covered asset digests and separately checked
  the checksum file's own digest against GitHub's API. Confirmed all 40 assets
  and draft/prerelease/source identity after upload.

The prepared release retains its exact source and validation run. Physical
M2–M5 acceptance remains open. The subsequent owner waiver below supersedes
the earlier physical publication gate for non-stable prereleases.

## Non-stable publication waiver and initial alpha publication

- [x] Owner approval: “lets bypass physical acceptance for the alpha release”,
  clarified as “waive it for non-stable releases”. Decision D-0001 records the
  general publication waiver for prerelease versions marked as GitHub prereleases.
  Stable publication and production deployment retain physical acceptance.
- [x] Policy and notes: preserve software/integrity gates and unverified M2–M5;
  update the roadmap, release guidance, changelog and public alpha notes.
  Tagged workflows continue preparing drafts; future publication still needs
  operator authorization.
- [x] Independent infrastructure and documentation reviews approved the scope
  and release-note qualification. No runtime or image build inputs changed.
- [x] Publication: `v0.1.0-alpha` published at `2026-10-12T02:46:13Z` as an
  immutable prerelease, not latest. Native release workflow `38193676481`
  remained successful; source stayed `f3921c9` and all 40 asset identities,
  sizes and SHA-256 digests matched before and after publication.

Hardware commissioning, RF testing and field rehearsal remain unperformed.
