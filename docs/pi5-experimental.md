# Experimental Raspberry Pi 5 NVMe Profile

The `mish-rpi5-nvme` host is a lab target for a Raspberry Pi 5 in an
Argon ONE-style case with an NVMe drive. It is intentionally non-release-gated:
the 0.1 SBC acceptance target remains Raspberry Pi 4.

This profile uses `github:nvmd/nixos-raspberrypi/main` for Raspberry Pi 5 boot
support and exposes:

- `nixosConfigurations.mish-rpi5-nvme`
- `packages.aarch64-linux.mish-rpi5-installer-image`
- `packages.aarch64-linux.mish-rpi5-nvme-system`

## Cache Setup

The flake declares the Raspberry Pi community cache in `nixConfig`, but the
first build on a new machine may need explicit command-line trust flags:

```bash
nix build .#packages.aarch64-linux.mish-rpi5-installer-image \
  --extra-substituters https://nixos-raspberrypi.cachix.org \
  --extra-trusted-public-keys nixos-raspberrypi.cachix.org-1:4iMO9LXa8BqhU+Rpg6LQKiGa2lsNh/j2oiYLNOQ5sPI=
```

The same flags are useful for `mish-rpi5-nvme-system` during first
evaluation or after switching Nix implementations.

## Installer Flow

Build the upstream Pi 5 installer image:

```bash
nix build .#packages.aarch64-linux.mish-rpi5-installer-image
```

Boot the installer on the Pi 5, prepare the NVMe drive with:

- a FAT firmware partition labeled `FIRMWARE`
- an ext4 root partition labeled `MISH`

Then install or switch to:

```bash
nixos-rebuild switch --flake .#mish-rpi5-nvme
```

The host configuration expects those labels and keeps the profile experimental.

## Determinate Nix Option

`services.mish.nixDistribution` records whether an appliance is intended
to run upstream Nix from NixOS or Determinate Nix. The Pi 5 profile defaults to
`"nixos"`. To mark a deployment as Determinate-managed after switching the host,
set:

```nix
services.mish.nixDistribution = "determinate";
```

Determinate Nix manages `/etc/nix/nix.conf`; custom settings should be placed in
`/etc/nix/nix.custom.conf`. When switching a Pi 5 test host for the first time,
include the Raspberry Pi cache there:

```text
extra-substituters = https://nixos-raspberrypi.cachix.org
extra-trusted-public-keys = nixos-raspberrypi.cachix.org-1:4iMO9LXa8BqhU+Rpg6LQKiGa2lsNh/j2oiYLNOQ5sPI=
```

This preserves the Pi 5 binary-cache path after Determinate takes over Nix
configuration. It does not make Pi 5 release-gated.
