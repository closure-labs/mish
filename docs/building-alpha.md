# Building and evidence levels

Build the event coordinator from the committed `flake.lock`. The supported
first-wave targets are x86_64 and Pi 4 (4 GB RAM); Pi 5 is experimental and
is outside their physical acceptance gate.

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
