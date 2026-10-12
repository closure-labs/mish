# Decision Log

## D-0001 — 2026-10-11 — Waive physical publication acceptance for non-stable releases

**Context:** `v0.1.0-alpha` completed native x86/ARM checks, x86 VM integration,
image builds and verification of all 40 uploaded assets. Physical M2–M5 tests
have not been performed.

**Decision:** The owner first requested publication of the alpha before physical
acceptance, then clarified: “waive it for non-stable releases”. Non-stable
prereleases may be published before physical acceptance. Publish the prepared
`v0.1.0-alpha` as a prerelease, retaining the tested tag/source and complete
verified assets. Record hardware and RF testing as unverified in release notes.

**Scope:** Alpha, beta, release candidate and other non-stable prereleases use
a prerelease version and GitHub's prerelease flag. Software, integrity and
access-control gates remain required. Physical milestones are still open;
stable publication and production deployment retain physical acceptance,
including deployment of a prerelease. Tag workflows still prepare drafts;
the waiver does not introduce automatic publication of future releases.

**Reversible:** Partial. Release notes can be corrected, but publication locks
the tag and assets under GitHub release immutability. Corrections to images or
code require a new release version.

**Owner:** @cto

**Related:** [Release qualification](https://github.com/closure-labs/mish/actions/runs/38193676481),
[alpha notes](../releases/v0.1.0-alpha.md), [physical acceptance](../acceptance.md).

## D-0002 — 2026-10-11 — Keep native queue rules and defer App setup

**Context:** The current merge queue and native checks have admitted and merged
maintainer PRs through native per-PR auto-merge. Unattended workflow admission
was designed around a separately configured GitHub App.

**Decision:** The owner confirmed the current merge-queue ruleset and deferred
App setup. Use native per-PR auto-merge with the maintainer's existing GitHub
session. Make App-based unattended admission explicitly opt-in and inactive
without `MISH_QUEUE_APP_AUTOMATION=true`.

**Reversible:** Yes. Configure and explicitly enable the optional App workflow
later if unattended admission is wanted.

**Owner:** @cto

**Related:** [Queue operation](../merge-queue.md), [PR #24](https://github.com/closure-labs/mish/pull/24).
