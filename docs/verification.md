# Implementation verification evidence

The sections below record historical revisions, including the earlier Python
runtime. Their Python commands and counts are evidence for those revisions,
not current contributor instructions. The current runtime is Rust; use
[contributing.md](contributing.md) for its checks.

## Latest product evidence

Channel enrollment merged in [PR #18](https://github.com/closure-labs/mish/pull/18)
as `b4dec1c`, followed by the documentation update in
[PR #19](https://github.com/closure-labs/mish/pull/19) as `a4660ac`. Its recorded
behavioral suite contains 68 Rust tests and seven Node 24 browser tests.
Native software and merge-group qualification passed; the x86 VM demonstrates
HTTPS, messages/alert ACKs, restart and no-replay recovery, plus the enrollment
landing page, an empty public channel catalog and protected-route rejection.
It does not demonstrate a fully provisioned radio mesh.

[Images run 38186639291](https://github.com/closure-labs/mish/actions/runs/38186639291)
passed on `b4dec1c` for x86 VM/installer and ARM SD artifacts. Mish has never
been deployed; physical Meshtastic app import, USB/RF operation, fresh-kit
commissioning, Pi 4 qualification and rehearsal remain unperformed. The Den
composition refactor passed native software/VM and image qualification for
`8b90573`; see the Den evidence below, [product status](product-status.md) and
[Den architecture](den-architecture.md).

The last recorded automatic queue configuration check found the GitHub App
variable and private-key secret absent. CLI queue admission exercised required
native merge-group checks; automatic admission still needs that setup.

## Rust runtime continuation

The initial Rust implementation passed 38 behavioral tests. It used Axum, Clap, a
bounded Tokio coordinator, SQLite and native Meshtastic serial framing. The
compiled appliance runtime contains no Python application package.

| Validation | Result | Scope |
|---|---|---|
| Rust tests, formatting and Clippy | Passed | 38 tests, including 7,776 generated delivery histories, typed HTTP access/input checks, radio framing/uncertainty and snapshot recovery |
| All-system Nix evaluation | Passed | Linux x86/ARM and Darwin evaluation; no cross-architecture realization implied |
| Native x86 Nix package and checks | Passed | Application source at `c04f529`; CLI negative coordinates, schemas, workflow lint and module policies |
| Booted x86 appliance VM | Passed | HTTPS trust, two staff identities, alert ACK, restart and snapshot restore without outbound replay; test script completed in 18.95 seconds |
| Compiled runtime closure | Passed | 10 store paths, including SQLite and stub shell wrappers; no Python runtime |
| Repository queue helper | Passed | Nine tests after incorporating the GitHub App admission correction |
| Native GitHub x86 and ARM checks | Passed on `1d7d68d` | [Checks run 38177392268](https://github.com/closure-labs/mish/actions/runs/38177392268), including the x86 VM |
| Rust appliance images | Passed for `e6cd187` | [Images run 38181091160](https://github.com/closure-labs/mish/actions/runs/38181091160) built both architectures; physical qualification remains open |

The automatic admission correction merged in
[PR #13](https://github.com/closure-labs/mish/pull/13). App registration,
repository-only installation and the documented Actions variable/secret remain
operator setup; an absent App configuration fails before queue mutation.
Physical USB/RF operation, Pi commissioning and rehearsal remain unperformed.

These results concern the implementation working tree on 2026-10-11. Release
automation must repeat checks against its tagged revision and record artifact
checksums. Physical acceptance remains separate.

| Evidence | Result | Scope |
|---|---|---|
| Python application tests in `nix develop` | 30 passed | Legacy behavior, event authorization/privacy/durability and radio simulation/failure regressions |
| All-system flake evaluation | Passed | Declared module/package/check expressions evaluate |
| Native Nix checks | Final run passed on x86 | Package, schema, formatting, CLI and service/module checks |
| x86 `vm-smoke` application test | Final run passed | Two staff sessions, messages, alert ACK, restart durability and protected/public reads in a booted appliance |
| Caddy local HTTPS in VM | Passed | Generated root certificate at `/var/lib/caddy/.local/share/caddy/pki/authorities/local/root.crt`; HTTPS verified with that trusted CA, without bypassing certificate validation |
| x86 QEMU appliance | Built | VM output realized; physical installation unverified |
| x86 installer ISO | Built | Final application/catalog snapshot realized, about 1.5 GB; physical installation unverified |
| Pi 4 SD image | Native ARM CI build passed on `a42ca31` | [Images run](https://github.com/closure-labs/mish/actions/runs/38170625479) also built the x86 VM and installer; physical boot and radio qualification remain unverified |
| Native GitHub x86 and ARM checks | Passed on `fabce7e` | [Checks run](https://github.com/closure-labs/mish/actions/runs/38171282570) validated the combined Dependabot updates and merge-queue trigger |
| Physical Meshtastic USB/RF | Unperformed | Requires mini-PC, two radios and WAN-disconnected acceptance |
| Eight-hour rehearsal | Unperformed | Requires resource/RF measurements and operator intervention record |

The automated checks establish behavior in their stated test environments.
They do not qualify hardware, venue coverage, power-loss recovery or event
operation. Use [acceptance.md](acceptance.md) to record those results.

The historical installer built before the Mish namespace rename is:

```text
/nix/store/bg485f8qvpflwhjph70wybrsz3bijgx8-nixos-minimal-26.05.20261010.6fcc160-x86_64-linux.iso/iso/nixos-minimal-26.05.20261010.6fcc160-x86_64-linux.iso
SHA-256: 752c8325b07b8577249acd3c937d5564acb000f99739993166e73892dc7463da
```

That source snapshot predates the current CLI/module namespace and subsequent
recovery work. It is retained as build evidence, not as the current Mish kit.
Use artifacts and manifests from the revision being qualified; tagged release
builds must generate their own manifests and checksums.

## Snapshot recovery continuation

The recovery implementation based on `fabce7e` was verified locally on
2026-10-11 using:

```bash
nix develop --command env PYTHONPATH=apps/mish/src python -m unittest discover -s tests
nix develop --command nix flake check --all-systems --no-build --accept-flake-config
nix develop --command nix flake check -L --accept-flake-config
```

All 38 Python tests passed, all-system evaluation passed, and all native x86
checks passed. The booted VM captured a pending message in a backup, allowed
the original coordinator to transmit it, restored the backup into fresh
state, and verified that the message remained uncertain while new traffic
transmitted. Staff authentication, alert ACK history, the explicit public
projection and trusted local HTTPS worked after restore. Independent security
and QA review approved the revised implementation after strengthening schema
compatibility checks. See [the recovery QA record](qa-reports/snapshot-recovery.md).

The Checks workflow repeats these gates natively on x86 and ARM and retains
evaluation/build logs with the tested source revision and architecture.
Recovery revision `c91498b` passed both native jobs in
[Checks run 38172407182](https://github.com/closure-labs/mish/actions/runs/38172407182).
The merged recovery revision `9c5d002` also passed native x86 and ARM image
realization in [Images run 38173136571](https://github.com/closure-labs/mish/actions/runs/38173136571),
including the x86 VM/installer and Pi 4 SD image. Physical acceptance remains
separate.

## Automatic merge queue continuation

The suite now contains 43 passing Python tests, including five queue-policy
tests. All-system evaluation and native x86 flake checks passed locally through
`nix develop`. Independent security review approved the automation after
restricting automatic admission to branches in this repository.

Queue automation revision `67c3347` passed both native GitHub jobs in
[Checks run 38173427998](https://github.com/closure-labs/mish/actions/runs/38173427998).
The queue's combined revision `fb6db84` passed both jobs in
[merge-group run 38173802058](https://github.com/closure-labs/mish/actions/runs/38173802058)
and [PR #8](https://github.com/closure-labs/mish/pull/8) merged. The active ruleset
requires both native checks, uses an ALLGREEN queue and grants no bypass actors.

This verification-record update is submitted without manual queue enrollment
to exercise the installed workflow. Its PR timeline and Merge queue Actions run
record automatic admission separately from the bootstrap PR's validation.
See [merge-queue.md](merge-queue.md) for eligibility, permissions and retry behavior.

## Shared cache and qualification gates

[PR #16](https://github.com/closure-labs/mish/pull/16) merged as `e6cd187`.
Main [Checks run 38181091031](https://github.com/closure-labs/mish/actions/runs/38181091031)
uploaded Mish, developer-shell and native check outputs to signed Cachix.
[Cold-cache run 38181529187](https://github.com/closure-labs/mish/actions/runs/38181529187)
passed on both native architectures with GitHub store restores disabled and
showed those outputs downloaded from `mish.cachix.org` without upload credentials.
[Images run 38181091160](https://github.com/closure-labs/mish/actions/runs/38181091160)
built both architectures and uploaded image closures. JavaScript actions use
Node 24 at pinned commits.

[PR #17](https://github.com/closure-labs/mish/pull/17) merged as `3061197`.
[Checks run 38183355942](https://github.com/closure-labs/mish/actions/runs/38183355942)
passed both native contexts and Software qualification. Active queue rules now
require all three; workflow dependency and release artifact validation fail
closed. Metadata jobs do not claim native validation snapshot keys.

## Community and private channel enrollment

The Rust feature has 68 passing behavioral tests and seven Node 24 channel
selection tests. The original 64-test suite passed in a single bounded run;
the four added USB deployment tests and strengthened malformed-plan regression
then passed separately. Strict Clippy and Rust/Nix formatting checks passed.
Every local command ran through `scripts/local-nix.sh` and `nix develop`, with
one compiler job and a 2 GiB process scope. Full all-system/VM/image validation
is delegated to native GitHub runners to protect the workstation.

The tests cover independent 32-byte keys, exact community/private QR scopes,
department read/write/ACK authorization, cross-channel radio ACK rejection,
durable channel fingerprints, restore re-verification and 501-record flood
visibility. Simulated USB fixtures exercise the production apply path for
successful readback, wrong settings, disconnect after mutation and unrelated
RF drift, with a protected recovery snapshot before each write.

Independent security review approved after fixing the staff browser's initial
channel selection and applying journal access filtering before its result
limit. Keys and RF settings are checked on radio connection and reported
configuration changes, not continuously against unreported external edits.
Physical Meshtastic app QR import, USB firmware interoperability and RF
communication remain unverified.

[Checks run 38185483836](https://github.com/closure-labs/mish/actions/runs/38185483836)
passed both native architectures, all-system evaluation, the booted x86 VM and
Software qualification for `b9ffef5` in
[PR #18](https://github.com/closure-labs/mish/pull/18). The VM exercises the actual
community enrollment landing page, channel-selection asset, anonymous private
route rejection and separation of private messages from the public catalog.

[Merge-group run 38186040176](https://github.com/closure-labs/mish/actions/runs/38186040176)
passed all three required contexts, and PR #18 merged as `b4dec1c`. Queue
admission used the authorized CLI path. The automatic admission workflow still
requires `MISH_QUEUE_APP_ID` and `MISH_QUEUE_APP_PRIVATE_KEY`; its
[configuration check](https://github.com/closure-labs/mish/actions/runs/38185948748)
reported both absent. Native merge-group validation ran without bypassing gates.

[Images run 38186639291](https://github.com/closure-labs/mish/actions/runs/38186639291)
then passed for merged feature revision `b4dec1c`, realizing the x86 VM and
installer plus the ARM SD image. [PR #19](https://github.com/closure-labs/mish/pull/19)
merged the documentation follow-up as `a4660ac`. Neither result closes the
physical channel-enrollment or event-kit gates.

## Den composition qualification

[PR #20](https://github.com/closure-labs/mish/pull/20) replaces direct host
construction with Den host/aspect composition. Den is pinned to the reviewed
reference revision `65c9fd40e958109f5a352c1a93ae8254bf1ed29f`. Existing public
output names and platform-specific settings remain; reusable modes preserve an
explicit host state version. The Rust application and database are unchanged.

[Checks run 38189634053](https://github.com/closure-labs/mish/actions/runs/38189634053)
passed all-system evaluation, both native Linux checks and Software
qualification for `8b90573`. The booted VM now imports the actual Den simulation
mode. Composition checks verify shipped defaults, state-version preservation
and the specific rejection assertions for placeholder/unstable devices, mixed
radio modes and wrong architecture. Initial qualification exposed an omitted
platform in the standalone architecture fixture; the fixture was corrected
without changing the production guard.

[Images run 38189631186](https://github.com/closure-labs/mish/actions/runs/38189631186)
passed x86 VM/installer and ARM SD realization plus Image qualification for
the same revision. Code, QA and security reviews approved. Local checks used
the one-job/2 GiB scope and targeted exports/configuration projections; full
evaluation, VM and image builds ran on GitHub. Physical M2–M5 acceptance remains
open.

[Merge-group run 38190366390](https://github.com/closure-labs/mish/actions/runs/38190366390)
passed both native contexts and Software qualification for the combined commit.
PR #20 merged as `8381b5f`; image evidence above remains attributed to its
qualified candidate `8b90573`.

## Initial alpha draft qualification

[PR #22](https://github.com/closure-labs/mish/pull/22) prepared the changelog,
curated alpha notes and guarded draft creation. Local validation ran 27 helper
tests in the bounded `nix develop` shell; seven release tests repeated after
final draft-state safeguards. Independent infrastructure, security and QA
reviews approved. [Native PR checks 38192810349](https://github.com/closure-labs/mish/actions/runs/38192810349)
and [merge-group 38193246186](https://github.com/closure-labs/mish/actions/runs/38193246186)
passed all required contexts; the PR merged as `f3921c9`.

Annotated tag `v0.1.0-alpha` points to
`f3921c9ce25990915d4d4320d51db4fda38d5c7c`.
[Draft release run 38193676481](https://github.com/closure-labs/mish/actions/runs/38193676481)
passed tag/version/notes checks, all-system evaluation, native x86/ARM checks,
the x86 VM gate and x86 VM/installer plus ARM SD image realization. The draft
gate verified checksums, expected payloads/targets, both lockfiles and source
identity before uploading.

Post-upload API verification confirmed a draft prerelease for that source and
40 complete assets totaling 2,953,428,530 bytes. Each GitHub-reported SHA-256
digest matches the downloadable aggregate `SHA256SUMS`, including the checksum
file's own separately computed digest. This verification downloaded only small
metadata and the checksum list to the workstation. It did not download or
physically boot the images. Physical M2–M5 acceptance and publication remain open.
