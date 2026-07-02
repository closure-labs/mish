# VestaForge

VestaForge 0.1 alpha is a NixOS appliance prototype for Mini-PC and
Raspberry Pi 4-class SBC targets.
It demonstrates profile-driven NixOS composition, Den-compatible quirk data,
Matrix provider selection, OpenZiti-protected local service surfaces, simulated
Meshtastic-like waypoint ingest, experimental Reticulum/RNode radio provider
wiring, signed GeoPack manifests, and mock/file Nostr publication.

The 0.1 release-gated targets are `x86_64-linux` with the `mini-pc` profile and
`aarch64-linux` with the `sbc-rpi4` profile. `home-server` is represented in the
catalog and option model, but is not release-gated by this alpha.

## Flake Outputs

- `nixosConfigurations.vestaforge-mini-pc`
- `nixosConfigurations.vestaforge-rpi4`
- `packages.x86_64-linux.{vestaforge-control,vestaforge-image,vestaforge-mini-pc-image}`
- `packages.aarch64-linux.{vestaforge-control,vestaforge-image,vestaforge-rpi4-image}`
- `packages.aarch64-darwin.vestaforge-control`
- `checks.x86_64-linux.{module-eval,matrix-dual-provider-policy,reticulum-rnode-provider,schema,cli,service-config,vm-smoke,vm}`
- `checks.aarch64-linux.{module-eval,matrix-dual-provider-policy,reticulum-rnode-provider,schema,cli,service-config}`
- `checks.aarch64-darwin.{schema,cli}`
- `devShells.x86_64-linux.default`
- `devShells.aarch64-linux.default`
- `devShells.aarch64-darwin.default`

## Local CLI

```bash
nix run .# -- status
nix run .# -- waypoint add-simulated --lat 44.0521 --lon -123.0868 --label "Checkpoint" --visibility public
nix run .# -- waypoint list
nix run .# -- geopack create --name "Field Pack" --waypoint <waypoint-id> --visibility public
nix run .# -- geopack publish --pack <pack-id>
nix run .# -- identity bootstrap-operator
```

By default, mutable state is under `/var/lib/vestaforge`. For local development,
set `VESTAFORGE_STATE_DIR` or pass `--state-dir`.

## NixOS Options

The appliance module exposes:

- `services.vestaforge.enable`
- `services.vestaforge.profile = "mini-pc" | "sbc-rpi4" | "home-server"`
- `services.vestaforge.domain`
- `services.vestaforge.matrix.provider = "tuwunel" | "conduit"`
- `services.vestaforge.openziti.enable`
- `services.vestaforge.geopack.enable`
- `services.vestaforge.nostr.relays`
- `services.vestaforge.radio.providers = [ "meshtastic-simulated" ] | [ "reticulum-rnode" ]`
- `services.vestaforge.radio.simulator.enable`
- `services.vestaforge.radio.reticulumRnode.enable`
- `services.vestaforge.radio.reticulumRnode.package`
- `services.vestaforge.radio.reticulumRnode.port`

Matrix providers are mutually exclusive for one domain. Enabling both Conduit
and Tuwunel produces a NixOS assertion failure.

`reticulum-rnode` is implemented as an experimental provider. It renders a
Reticulum `RNodeInterface` configuration and starts `rnsd`, but nixpkgs marks
the RNS package as unfree in the current pin. Deployments that select this
provider must explicitly allow the package and set
`services.vestaforge.radio.reticulumRnode.package = pkgs.rns`.

## HTTP API

The control API binds to `127.0.0.1` and is intended to be exposed remotely only
through the OpenZiti service model:

- `GET /health`
- `GET /waypoints`
- `POST /waypoints/simulated`
- `POST /geopacks`
- `POST /geopacks/{id}/publish`

## Verification

```bash
nix flake check
```

From non-Linux development hosts, include every declared Linux target explicitly:

```bash
nix flake check --no-build --all-systems
```

Build entrypoints:

```bash
nix build .#packages.x86_64-linux.vestaforge-mini-pc-image
nix build .#packages.aarch64-linux.vestaforge-rpi4-image
```

On an AArch64 Darwin development host, use the build status helper to run the
host-native checks and Linux image dry-runs:

```bash
scripts/alpha-build-status.sh
```

After configuring a Linux builder, require the actual Mini-PC and RPi4 Linux
builds:

```bash
scripts/alpha-build-status.sh --full
```

If Nix reports `builders = @/etc/nix/machines` but `/etc/nix/machines` is
missing, no Linux builder is currently available to realize those Linux outputs
from Darwin.

See `docs/building-alpha.md` for local readiness levels and Darwin Linux builder
notes.
