# Mish / VestaForge

Mish is an offline, owner-operated event coordination appliance. VestaForge is
its existing repository, NixOS module and CLI name. One coordinator serves a
private staff web app, journals messages and alerts in SQLite, and connects to a
USB Meshtastic radio. Attendees see a read-only feed of explicitly published
announcements. An external Wi-Fi access point connects to the coordinator by
Ethernet; event operation requires no cloud account or WAN.

The initial target is x86_64, followed by Raspberry Pi 4 with 4 GB RAM. The Pi 5
and home-server profiles remain experimental. This is an implementation
prototype: image realization, physical installation, radio interoperability and
an eight-hour event rehearsal require separate acceptance evidence.

## Start here

- [Commissioning and recovery](docs/commissioning.md): installation, local TLS,
  staff/radio enrollment, backup, restore and end-of-event wipe.
- [Live event operator runbook](docs/operator-runbook.md): outages, uncertain
  sends, unacknowledged alerts and recovery.
- [Architecture and milestone gates](docs/roadmap.md): capability inventory,
  dependencies, acknowledgement semantics and deferred work.
- [Verification evidence](docs/verification.md): automated results and open
  image/hardware gates.
- [Physical acceptance record](docs/acceptance.md): reproducible field checklist
  and measured results; unperformed checks remain open.
- [Contributing](docs/contributing.md): local checks, jj bookmarks and PRs.
- [Building](docs/building-alpha.md): native Linux outputs and macOS builders.

## Build and verify

```bash
nix develop
nix flake check --no-build --all-systems
nix flake check
nix build .#packages.x86_64-linux.vestaforge-mini-pc-image
nix build .#packages.aarch64-linux.vestaforge-rpi4-image
```

The mini-PC image name remains compatible with earlier CLI/module names and
builds a QEMU VM. Physical installation uses the separate installer output
described in the commissioning guide. Evaluation checks expression validity;
builds realize artifacts; VM tests exercise a running application. None proves
physical hardware readiness. Build ARM outputs on a native ARM builder.

The supported-target configuration uses NixOS 26.05 and `system.stateVersion =
"26.05"`: the project has never been deployed, so there is no installed-state
compatibility to preserve. Inputs are committed in `flake.lock`.

## Application boundaries

The HTTP application binds to loopback by default and sits behind Caddy local
HTTPS on the appliance. Operator and staff bearer credentials are generated at
commissioning or first boot and kept in runtime state, never in shared images.
The local CLI runs as the appliance's trusted operator/service user; filesystem
access to that state is administrative access.

Messages, alerts and acknowledgement history are private. Publication is an
explicit operator action that creates a separate announcement projection.
Incoming radio messages never publish themselves. Broadcast transport
confirmation does not prove a human received or read an alert; staff must
acknowledge it explicitly.

The existing waypoint/GeoPack CLI is retained for compatibility. Matrix and
OpenZiti services are health stubs, not implemented messaging or access
protection. Reticulum starts a daemon but has no application bridge. Nostr
mock/file publication and waypoint simulation are development facilities;
these integrations are deferred and simulations are opt-in. The
[historical whitepaper](docs/vf-whitepaper-draft.md) is retained as a design
archive and does not describe current delivered capabilities.

## HTTP surface

Send `Authorization: Bearer <token>` for staff/operator operations. The bundled
staff app uses the same routes. Requests are bounded to 16 KiB; text on the
initial radio path is limited to 160 UTF-8 bytes.

| Route | Access | Behavior |
|---|---|---|
| `GET /health` | Public | Minimal coordinator health |
| `GET /staff` | Public app shell | Data requires an enrolled token |
| `GET /public` and `/public/announcements` | Public | Published announcements only |
| `GET /api/status` | Staff | Selected adapter and current radio connection state |
| `GET /api/messages` | Staff | Private journal, enrolled author names and acknowledgement state |
| `POST /api/messages` | Staff | Durably accept text into the outbox |
| `POST /api/alerts` | Staff | Accept alert with responder and expiry |
| `POST /api/alerts/{id}/ack` | Staff | Explicit authenticated acknowledgement |
| `POST /api/staff` | Operator | Generate staff/operator enrollment token |
| `POST /api/announcements` | Operator | Explicitly publish announcement text |
| `DELETE /api/staff/{id}` | Operator | Revoke a credential, preserving the last operator |

The original waypoint/GeoPack HTTP operations require operator authorization;
network GeoPack publication remains a local CLI operation during the PoC.

The journal preserves delivery timestamps separately from its current queue,
submission, transport-confirmation and uncertainty state. Human ACKs remain
separate records. Real-radio sends are paced by
`services.vestaforge.radio.meshtastic.sendIntervalSeconds` (30 seconds by
default). This is engineering pacing, not an RF delivery-time promise.
