# Changelog

User-facing changes in Mish. `0.1.0-alpha` is the initial prerelease.

## Unreleased

### Added

- Offline attendee chat at `/public`: choose a nickname, join or leave rooms,
  read retained history and send text. Rooms can stay on Wi-Fi or connect to a
  verified Meshtastic community channel. Private mapped rooms remain limited
  to authorized staff, and community radio QR enrollment stays available.
- Separate, revocable guest sessions and operator controls to create or archive
  rooms, hide messages, pause enrollment and set posting limits. Guest enrollment
  is off until an operator enables it. See [community rooms](docs/community-rooms.md).
- Durable room history with duplicate-safe submission retries and inclusion in
  backup, restore, retention and wipe. Browser drafts survive reloads and
  disconnects; reconnecting requires an explicit Send. Messages allow up to
  160 UTF-8 bytes, including the author label when sent over radio.
- Event-domain HTTPS using externally supplied certificates, plus an optional
  TCP connection to `meshtasticd`. Local-CA HTTPS remains available and USB
  Meshtastic remains the default. See [event connectivity](docs/event-connectivity.md).
- Standalone Nix packages and services for Supply Drop BBS and experimental
  Reticulum/RetiBBS, with separate state and identities. Examples cover dedicated
  Meshtastic radios, a MeshCore-only BBS and a shared Reticulum daemon with optional
  RNode hardware. These services can run independently of Mish; MeshCore does not
  carry Mish rooms or alerts. See [optional mesh services](docs/optional-mesh-services.md)
  for setup and upstream license requirements.

### Changed

- Guest radio queues are bounded and staff transmissions take priority. Wi-Fi
  activity does not consume radio airtime unless a room is explicitly mapped.
- Mish returns to Apache 2.0 (`Apache-2.0`). The repository, Rust crate,
  Nix package and future release kits include the license text. Published
  release tags and artifacts are unchanged.

### Qualification

Native x86/ARM checks, VM scenarios and appliance image builds passed. Separate
[one-hour HTTP and Chromium tests](https://github.com/closure-labs/mish/actions/runs/38247261916)
each supported 50 concurrent sessions and 3,600 messages, with no accepted-message
loss and updates visible within five seconds. This measures local chat capacity,
not radio throughput. Physical iOS/Android, event certificate/DNS and radio
acceptance remain open; use the [acceptance record](docs/acceptance.md).

## [0.1.0-alpha] — 2026-10-11

An owner-operated coordinator for offline event communication, with community
Meshtastic enrollment and private staff/department coordination. Software and
image builds are qualified at the revisions in the
[verification record](docs/verification.md). Physical installation, radio
interoperability and field acceptance remain open.
The owner waived physical acceptance as a publication gate for non-stable
prereleases. Hardware/RF qualification remains open for this alpha.

### Added

- Authenticated staff browser messaging, operational/urgent alerts with a
  responder and expiry, and explicit human acknowledgements. Durable acceptance,
  radio submission, transport confirmation and uncertainty remain distinct.
- Community mesh generation or existing PRIMARY-channel import, public
  community QR enrollment and private staff/department QR exports. Generated
  keys are full 256-bit keys; private exports are scoped to authorized membership.
- Channel-scoped browser messages, journal reads and acknowledgements, with
  radio replies retaining their node and channel provenance.
- Explicit USB radio provisioning with a protected recovery snapshot and
  verified readback. Reconnecting the adapter does not rewrite radio settings.
- Local HTTPS, operator/staff enrollment, credential revocation, seven-day
  retention and an explicit end-of-event wipe.
- Consistent SQLite backup and fresh-state restore. Pending snapshot sends
  become uncertain to prevent automatic replay after recovery.
- x86 QEMU appliance and installer ISO, plus a Pi 4 SD-image target, with
  revision manifests, checksums and commissioning/recovery documentation.

### Changed

- The application, CLI, NixOS module, service names and runtime paths now use
  Mish. Scripts and configuration based on earlier prototype names need updating.
- The appliance runtime is Rust-native; its HTTP service, CLI, storage and
  serial adapter no longer require a Python application package.
- Public participation uses the Meshtastic app. The public browser provides
  community QR enrollment; authenticated staff browser access is membership-scoped.
  Earlier announcement routes remain deprecated compatibility facilities.
- NixOS 26.05 and pinned inputs define appliance builds. Den composes modes,
  features and architectures while preserving the current Mish interfaces.

### Fixed

- Snapshot recovery no longer automatically retransmits messages that the
  original coordinator may have sent after backup capture.
- Staff channel selection and journal filtering preserve private-channel
  boundaries, including radio acknowledgements from other channels.
- Invalid identifiers, malformed/oversized requests and unauthorized private
  operations are rejected. Public enrollment exports exclude private keys.

### Alpha limits

Mish has never been deployed. Physical M2–M5 gates are unqualified, including
Meshtastic-app QR import, USB/firmware interoperability, fresh installation,
Pi 4 commissioning, resilience and an eight-hour rehearsal. Radio coverage,
latency, memory and commissioning time remain measurements to collect.

One coordinator is authoritative; spares are activated manually. A copied
radio key is not revoked by withdrawing a web token or membership: removing RF
access requires key rotation and radio re-enrollment. Matrix/OpenZiti are
optional health stubs; additional transports, replication and automatic
failover remain deferred.

See the [alpha notes](docs/releases/v0.1.0-alpha.md),
[product status](docs/product-status.md) and
[physical acceptance record](docs/acceptance.md).
