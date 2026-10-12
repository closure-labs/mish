# Building and evidence levels

Build the event coordinator from the committed `flake.lock`. The supported
first-wave targets are x86_64 and Pi 4 (4 GB RAM); Pi 5 is experimental and
is outside their physical acceptance gate.

The appliance package uses Nix `buildRustPackage` with the committed
`apps/mish/Cargo.lock`. Formatting, Clippy and Cargo tests are package checks.
Python supports repository automation; it is not an application runtime
dependency. Earlier image evidence does not substitute for building and
testing the Rust port revision.

```bash
nix flake check --no-build --all-systems
nix flake check
nix build .#packages.x86_64-linux.mish-control
nix build .#packages.x86_64-linux.mish-mini-pc-image
nix build .#packages.x86_64-linux.mish-mini-pc-installer
nix build .#packages.aarch64-linux.mish-rpi4-image
```

The mini-PC image output is a QEMU VM. The separate mini-PC installer output
is a bootable ISO with installation/rescue tools; it does not automatically
partition or erase a disk. The Pi 4 output is an SD image. See
[commissioning.md](commissioning.md) for physical setup and
[acceptance.md](acceptance.md) for qualification.

| Evidence | Establishes | Does not establish |
|---|---|---|
| All-system evaluation | Expressions and option assertions evaluate | Packages/images build |
| Native package/check builds | Artifacts realize on that architecture | Images boot or radios work |
| x86 VM integration | Services boot and actual application operations pass in QEMU | Physical installation/USB/RF readiness |
| Image builds | VM/ISO/SD artifacts exist from pinned sources | Hardware-specific boot and recovery |
| Physical acceptance | Recorded procedures pass on named hardware | Other boards, venues or RF conditions |

## Linux builders from macOS

The helper `scripts/alpha-build-status.sh` runs Darwin-native checks and Linux
image dry-runs; `--full` also attempts Linux artifact builds. Its report should
be interpreted according to the evidence levels above.

A macOS host requires a Linux-capable builder for each architecture. A native
ARM Linux builder realizes `aarch64-linux`; an x86 Linux builder realizes
`x86_64-linux`. CI uses native Linux runners for both.

If using Determinate Nix's native Linux builder, inspect
`determinate-nixd status`, `determinate-nixd version` and
`nix config show external-builders`. Access to that development builder may
require a hosted login; deployed event operation does not.

Nixpkgs' macOS VM builder can provide ARM Linux builds:

```bash
nix build nixpkgs#darwin.linux-builder --no-link --print-out-paths
```

Its `create-builder` program creates a VM and SSH material and may install
keys under `/etc/nix` through sudo. Keep it running and configure a builder
entry appropriate to its advertised architecture. It cannot satisfy an x86
build unless it explicitly supports x86 execution. Inspect
`nix config show builders` before attempting full builds. Avoid interpreting
`required system or feature not available` as an application failure: it means
no suitable builder was available.

## Artifact records

Image CI uploads SHA-256 checksums, source revision and a build manifest. Verify
those checksums before flashing. A draft tagged release must rebuild from its
tag and include commissioning instructions. Record any skipped or blocked
build/VM check explicitly; do not describe evaluation-only verification as a
successful image build.

## Preparing a tagged alpha

The initial candidate is `v0.1.0-alpha`, matching the Rust manifest, Cargo
lockfile and Nix package version. Update [CHANGELOG.md](../CHANGELOG.md) and
the corresponding file under `docs/releases/` together. Native flake checks
exercise release-helper regressions; the tagged qualifier also refuses a
version mismatch or missing notes before building images.

Commit and merge the candidate through the required native checks and merge
queue. From that exact clean main revision, create an annotated tag and push
it once; do not move a release tag to repair a candidate. If source changes
are needed, prepare a new prerelease version and tag.

```bash
bash scripts/local-nix.sh --command python .github/scripts/release_metadata.py --tag v0.1.0-alpha
bash scripts/local-nix.sh --command git tag -a v0.1.0-alpha -m 'Mish initial alpha qualification candidate'
bash scripts/local-nix.sh --command git push origin v0.1.0-alpha
```

The **Draft release** workflow repeats all-system evaluation, native Linux
checks, x86 VM behavior and image builds from the tagged revision. Its final
gate verifies downloaded checksums, architecture, source and lockfile. It
attaches the x86 VM closure/installer, Pi 4 SD image, validation manifests,
source revision, lockfile, operating docs, changelog and curated release notes.
Each architecture's checksum list uses the attached asset names; the aggregate
`SHA256SUMS` additionally covers the standalone documentation and lists.

The result is a draft prerelease and is not marked latest. A partial upload
can be resumed by rerunning the draft job only for the same tag and revision;
the helper refuses published releases or a different draft identity. Draft
creation and upload have a repository-scoped write token only after validation.

Download the assets together and run `sha256sum --check SHA256SUMS` before
using media. Retain the tagged source and validation run URL with the physical
acceptance record. GitHub validation establishes software behavior and realized
images; it does not establish hardware or RF readiness.

Publication remains a separate operator action after the required physical
acceptance evidence is recorded for stable releases. The owner waived this gate
for non-stable prereleases, including alpha, beta and release candidates. Use
a prerelease version and GitHub's prerelease flag, retain software and integrity
checks, and state unverified hardware/RF testing in the release notes. This
waiver does not qualify physical milestones or authorize production deployment.
Tag automation continues to prepare drafts; publication requires operator
authorization. Stable releases and production deployment retain the physical gate.
Release immutability makes publication the
point after which image assets and the release tag cannot be replaced. See
[GitHub's release guidance](https://docs.github.com/en/repositories/releasing-projects-on-github/managing-releases-in-a-repository).
