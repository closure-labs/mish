# Optional standalone mesh services

Mish's `nixosModules.mesh-services` has no Mish or Den dependency. Import it to
use `services.mesh-services.supplyDrop`, `.reticulum`, and `.retiBBS` independently.
The flake exports `packages.<linux-system>.supply-drop-bbs`, `retibbs`, and
`reticulum` for x86_64 and aarch64. Examples live in `examples/mesh-services`.
Mish imports the generic module only to reserve its radio device and reject
conflicting owners. These exports can later move together to one mesh-services
flake without changing the applications.

Meshtastic remains Mish's supported default. Supply Drop's MeshCore profile is
a standalone radio BBS, not a Mish room/alert adapter. RetiBBS is experimental,
with independent identities, rooms and ephemeral chat. There is no automatic
cross-protocol bridge. Neither BBS replaces attendee browser chat at `/public`.
mesh-api, Thinkleet and an unverified Reticulum flake remain deferred.

## Pins and licenses

| Component | Source pin | License |
|---|---|---|
| Supply Drop | 1.1.6 / `470df11119cc05b0b5d7530f246855829ca5a7a1` | Apache-2.0 with Commons Clause, not OSI open source |
| RetiBBS | `7c26d2c05e945f67382695d2f8de21abdb6d7020` | MIT |
| Reticulum / LXMF | 1.2.9 / 0.9.8 from Mish's locked nixpkgs | Reticulum License, marked unfree by nixpkgs |
| Bootstrap assets | 5.3.3, content-hashed CSS/JS | MIT |

Supply Drop 1.1.6 supports the pinned Rust compiler; later inspected releases
require Rust 1.96. Cargo and npm lockfiles are copied verbatim from the pinned
source; generic dependencies come from the flake's pinned nixpkgs. The package
builds and embeds the operator frontend offline. RetiBBS's CDN Bootstrap assets
are pinned and served locally. Packages retain upstream LICENSE and Supply
Drop NOTICE files; Bootstrap retains its embedded notice. Examples explicitly
allow the relevant nonfree packages. Commercial licensing arrangements are
outside this implementation; the license metadata is not replaced or relaxed.

Upstreams: https://github.com/Mesh-America/supply-drop-bbs and
https://github.com/kc1awv/RetiBBS .

## Operation

Supply Drop requires a dedicated stable `/dev/serial/by-id/…` radio path.
Duplicate assignments with Mish, the legacy RNode service or the shared RNode
are rejected during module evaluation. Distinct aliases to the same hardware
must also be avoided by the operator. Replace all example placeholders before
enabling a profile. Administration binds to loopback port 8080 when enabled;
use an operator SSH tunnel. Create its separate sysop account using upstream
`supply-drop-bbs --data-dir /var/lib/supply-drop user create --sysop NAME`
as the supply-drop user. Password prompts stay outside the Nix store. The
configuration is declarative and read-only; change radio/settings via Nix,
not the web Settings editor. The UI's user/board administration remains
separate from Mish. Its Secure cookies are retained; use a trusted HTTPS proxy
or a browser's secure localhost handling for the operator tunnel.

The shared Reticulum daemon owns LAN interfaces and the optional RNode. RetiBBS
runs as another user and requires the shared daemon, using the loopback TCP
shared-instance interface with its own state/config directory. It cannot fall
back to becoming a hardware-owning daemon. Disable the legacy Mish RNode service
when enabling this shared service. `reticulum.openFirewall` optionally opens
LAN discovery/data UDP ports; keep the shared/control TCP ports private. RNode
frequency and modem settings must be selected and physically qualified for the
event. The default example enables only LAN interfaces.

Persistent directories are `/var/lib/supply-drop`, `/var/lib/mesh-reticulum`,
and `/var/lib/retibbs`, with separate system users and owner-only permissions.
These services have independent restart policies and no dependency from Mish.
RetiBBS's optional read-only web server binds to 127.0.0.1:5000. Its systemd
adaptation removes interactive stdin waiting and requires the shared instance;
upstream chat and boards logic is unchanged. Credentials and identities are
created in state, never embedded in images. Back up these directories separately
while services are stopped; Mish's journal backup/wipe does not own BBS state.

## Qualification

Both local native package builds passed, including 24 Supply Drop tests and
RetiBBS imports. Direct local startup/restart smoke tests passed for both BBS
services, including Reticulum shared-instance connectivity, persistent BBS
identity, local operator UI and disconnected-radio isolation. Standalone module checks
cover missing-daemon and duplicate-device rejection, distinct service users,
and independence from Mish. Native CI builds both packages on both architectures
and runs the x86 service startup/restart/persistence/failure-isolation VM.
Package, VM and physical evidence are distinct: no physical MeshCore, RNode or
native Linux Meshtastic hardware qualification is implied by package success.
Record each experimental hardware profile independently before field use.
