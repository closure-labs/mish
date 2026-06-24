# Building the Alpha

VestaForge 0.1 has two release-gated appliance targets:

- `x86_64-linux` Mini-PC: `packages.x86_64-linux.vestaforge-mini-pc-image`
- `aarch64-linux` Raspberry Pi 4-class SBC: `packages.aarch64-linux.vestaforge-rpi4-image`

On an `aarch64-darwin` development host, there are three useful verification
levels.

## Local Readiness

Run:

```bash
scripts/alpha-build-status.sh
```

This performs:

- all-system flake evaluation with `nix flake check --no-build --all-systems`
- explicit profile assertions for Mini-PC and RPi4
- host-native Darwin package, schema, and CLI checks
- Linux image dry-runs for both release-gated targets

This is the strongest check that does not require Linux builders.

## Full Alpha Build

Run:

```bash
scripts/alpha-build-status.sh --full
```

This requires a Linux-capable builder path for both Linux systems:

- `x86_64-linux`
- `aarch64-linux`

The builder path can be either:

- Determinate Nix's native Linux builder for macOS, exposed through Nix
  `external-builders`
- classic Nix distributed builders in `/etc/nix/machines`

If no Linux-capable builder is available, Nix exits with `required system or
feature not available`.

## Determinate Native Builder

On macOS with Determinate Nix, prefer Determinate's native Linux builder. The
expected host signals are:

```bash
determinate-nixd status
determinate-nixd version
nix config show external-builders
```

The Determinate nix-darwin module should render:

```json
{
  "builder": {
    "state": "enabled"
  }
}
```

at `/etc/determinate/config.json`.

The native builder is access-gated by Determinate/FlakeHub. If Linux builds fail
while `/etc/determinate/config.json` has the builder enabled, check that:

- `determinate-nixd status` shows a logged-in FlakeHub user
- `determinate-nixd version` lists the native builder among enabled features
- Determinate Nix is current enough for the native builder
- the daemon has been restarted after any update

Determinate's troubleshooting flow is:

```bash
determinate-nixd auth logout
determinate-nixd auth login
sudo launchctl kickstart -k system/systems.determinate.nix-daemon
```

## Nixpkgs VM Builder Fallback

Nixpkgs also provides a macOS Linux builder VM:

```bash
nix build nixpkgs#darwin.linux-builder --no-link --print-out-paths
```

The helper's main program is `create-builder`. Running it creates SSH key
material, may install `/etc/nix/builder_ed25519*` through `sudo`, creates a
`nixos.qcow2` disk image, and starts an `aarch64-linux` NixOS VM with SSH
forwarded on host port `31022`.

Keep the VM running in a separate terminal. Then configure Nix with a builder
entry for `aarch64-linux`, for example in `/etc/nix/machines`:

```text
ssh-ng://builder@localhost:31022 aarch64-linux - 4 1 benchmark,big-parallel -
```

That handles the Raspberry Pi image target. It does not satisfy the Mini-PC
target unless the builder also advertises and can execute `x86_64-linux`.
For `--full` to pass without Determinate's native builder, also provide a
builder that advertises `x86_64-linux`, such as a local or remote x86_64 Linux
Nix builder, or replace the Mini-PC build strategy with a deliberate cross-build
design.

After updating builder configuration, restart or reload the Nix daemon as
appropriate for the host, confirm either `nix config show external-builders`
or `nix config show builders` exposes a Linux-capable path, and rerun:

```bash
scripts/alpha-build-status.sh --full
```
