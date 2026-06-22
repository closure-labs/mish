# VestaForge

VestaForge 0.1 alpha is a NixOS appliance prototype for the Mini-PC target.
It demonstrates profile-driven NixOS composition, Den-compatible quirk data,
Matrix provider selection, OpenZiti-protected local service surfaces, simulated
Meshtastic-like waypoint ingest, signed GeoPack manifests, and mock/file Nostr
publication.

The 0.1 release-gated target is `x86_64-linux` with the `mini-pc` profile.
`sbc-rpi4` and `home-server` are represented in the catalog and option model,
but are not release-gated by this alpha.

## Flake Outputs

- `nixosConfigurations.vestaforge-mini-pc`
- `packages.x86_64-linux.vestaforge-control`
- `packages.x86_64-linux.vestaforge-image`
- `checks.x86_64-linux.{module-eval,matrix-dual-provider-policy,schema,cli,service-config,vm-smoke,vm}`
- `devShells.x86_64-linux.default`

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
- `services.vestaforge.radio.simulator.enable`

Matrix providers are mutually exclusive for one domain. Enabling both Conduit
and Tuwunel produces a NixOS assertion failure.

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

For faster evaluation-only feedback:

```bash
nix flake check --no-build
```
