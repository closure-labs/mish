# Event acceptance record

Copy this record for each candidate release. Leave unchecked tests open until
run; a workflow definition or image build is not physical acceptance evidence.

- Revision / tag:
- Image SHA-256 and manifest:
- Test date / operator:
- Hardware model, RAM, disk / SD card, power supply:
- Radio models, firmware, region, channel index and modem preset (omit keys):
- AP model / configuration / coordinator address:
- Logs, measurements and defect links:

## Automated evidence

Record command, architecture and pass/fail result separately for:

- [ ] Rust behavioral tests, formatting/Clippy and schema/example validation.
- [ ] Non-mutating formatting and all-system flake evaluation.
- [ ] Native x86 and ARM packages/checks built from the committed lockfile.
- [ ] x86 VM boot and application interaction passed.
- [ ] x86 VM/installer and Pi 4 SD image realized; checksums verified.
- [ ] Traversal, malformed/oversized requests, unauthorized private reads and writes rejected.
- [ ] Private messages, identities, positions and unpublished alerts absent from public responses.
- [ ] Queued messages and acknowledgement history survive service restart.
- [ ] Snapshot recovery preserves channel keys, memberships, credentials and ACKs; radio verification is reset and pending sends remain uncertain.
- [ ] Corrupt/incompatible backups and existing restore targets are rejected without overwriting state.
- [ ] Duplicate packets, disconnects, uncertain sends, expired alerts and exhausted queues do not report false success.

## Physical commissioning

- [ ] Begin with a fresh x86 installation and external Ethernet-connected AP.
- [ ] Disconnect WAN before enrollment and first message.
- [ ] Join an existing primary community channel or generate the regional mesh; deploy and verify the coordinator's private staff/department channels.
- [ ] Verify stable USB path and explicit service permissions.
- [ ] Trust local CA on two staff devices; enroll distinct staff identities.
- [ ] Exchange browser text; create alert with owner and expiry; acknowledge as another staff member.
- [ ] Two physical radios exchange text through the adapter and send `ACK <ID>` with recorded node/channel provenance.
- [ ] Public QR enrolls two community radios that exchange text without WAN; its decoded configuration contains no private keys.
- [ ] Staff and each department QR enroll the intended private channels; unauthorized browser/private exports fail and community radios cannot receive private traffic.
- [ ] Record elapsed commissioning time against the 30-minute target.
- [ ] Repeat on Pi 4 with 4 GB RAM before qualifying the ARM target.

## Resilience

- [ ] Disconnect/reconnect USB; outbox resumes; uncertain sends remain visible.
- [ ] Lose LAN connectivity, reconnect and reconcile durable records.
- [ ] Restart application and reboot appliance; history and queues persist.
- [ ] Interrupt power after durable acceptance; accepted records survive.
- [ ] Restore a backup onto a spare; verify scoped journal, membership and ACK history, then independently verify its radio configuration.
- [ ] Review quarantined snapshot sends and demonstrate new messages transmit after recovery.
- [ ] Roll back a software generation; verify runtime/database compatibility.
- [ ] Stop coordinator; two staff radios still communicate directly.
- [ ] Verify expired alerts and bounded queues under actual radio congestion.
- [ ] Export event records and test explicit end-of-event wipe, including credentials and CA material.

## Eight-hour rehearsal

Use 10–30 staff and up to 20 radios, one active coordinator, representative
venue paths and optional manually activated spare. Record measurements rather
than assuming targets were met.

| Measure | Method / sampling interval | Result |
|---|---|---|
| Delivery latency distribution | Timestamp sent and received per path; separate browser and RF | Pending |
| Unacknowledged alerts | Alert owner, expiry, browser/radio provenance | Pending |
| Congestion / loss / duplicates | Radio packet and adapter logs; count retries/uncertainty | Pending |
| Application memory | Service cgroup memory measurements; target under 1 GB | Pending |
| Storage growth | SQLite and complete state directory before/after | Pending |
| Power and temperatures | Meter and device sensor samples | Pending |
| Operator intervention | Time, symptom, action and outcome | Pending |
| Confirmed durable acceptance | Compare accepted IDs before/after failures | Pending |

## Release decision

- Open defects and operational limitations:
- Hardware approved (x86, Pi 4 separately):
- Operator runbook corrections:
- Acceptance evidence reviewer:
- Draft release readiness decision:

Do not advertise RF coverage, latency, physical installation success or recovery
readiness until the corresponding measurements and procedures above have been
completed on the release revision.
