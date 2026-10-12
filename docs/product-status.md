# Mish product status and next acceptance gates

Mish has a software-validated offline coordinator and event-kit artifacts. It
has never been deployed. M0 and M1 software gates have passed; M2 through M5
remain open because physical commissioning, radio operation and rehearsal have
not been performed. The [roadmap](roadmap.md) defines their acceptance gates.

The product remains one owner-operated coordinator, an external Ethernet
access point, authenticated staff browsers and USB-connected Meshtastic radio.
Community participants use the Meshtastic app; the public browser offers
community QR enrollment. Staff and department browser access requires
authentication and channel membership. Additional coordinators are manually
activated spares.

## Recorded software and artifact evidence

Channel enrollment merged in [PR #18](https://github.com/closure-labs/mish/pull/18)
as `b4dec1c`. Documentation follow-up
[PR #19](https://github.com/closure-labs/mish/pull/19) merged as `a4660ac`.
The [verification log](verification.md) retains historical results and their
source revisions.

| Evidence | Recorded result | Limit |
|---|---|---|
| Rust and browser behavior | 68 Rust tests and seven Node 24 channel-selection tests passed | Simulated USB fixtures establish software behavior, not firmware interoperability |
| Native software qualification | Both architectures and all three required merge-group contexts passed for PR #18 | ARM native builds do not establish Pi 4 boot or USB support |
| x86 VM application integration | Trusted HTTPS, authenticated text/alert ACK, restart and snapshot recovery passed; enrollment page and protected routes exercised | The channel catalog is empty in this VM scenario; it does not exercise a fully provisioned event mesh |
| Event-kit image realization | [Images run 38186639291](https://github.com/closure-labs/mish/actions/runs/38186639291) passed on `b4dec1c`, building x86 VM/installer and ARM SD artifacts | Image realization does not establish physical installation, boot, RF or recovery |
| Den aspect restructuring | Native software, Den-composed VM and image qualification passed for `8b90573` in [PR #20](https://github.com/closure-labs/mish/pull/20) | Build qualification does not close physical gates |

The [Den architecture](den-architecture.md) describes the composition change.
Its gate is preservation of the public flake, NixOS module, application and
image interfaces with the same application behavior.

[Den Checks run 38189634053](https://github.com/closure-labs/mish/actions/runs/38189634053)
passed both native architectures and Software qualification.
[Den Images run 38189631186](https://github.com/closure-labs/mish/actions/runs/38189631186)
passed both architecture builds and Image qualification. The new composition
checks cover shipped defaults, reusable state-version preservation and specific
invalid-device, mixed-radio and wrong-architecture rejection paths.

All three required contexts passed in
[Den merge-group run 38190366390](https://github.com/closure-labs/mish/actions/runs/38190366390).
PR #20 merged as `8381b5f`. The composition refactor is delivered; the remaining
product acceptance work below requires hardware and field evidence.

## Initial alpha draft

`v0.1.0-alpha` is prepared at tagged source `f3921c9`, delivered through
[PR #22](https://github.com/closure-labs/mish/pull/22). The
[tagged release workflow](https://github.com/closure-labs/mish/actions/runs/38193676481)
passed native x86/ARM checks, the x86 VM gate and VM/installer/SD image builds.
The [draft prerelease](https://github.com/closure-labs/mish/releases/tag/untagged-6a68b87966db266145a7)
contains 40 assets; their uploaded SHA-256 digests match the aggregate checksum
manifest. Source, tag and draft identity were checked separately after upload.

See the [changelog](../CHANGELOG.md) and [alpha notes](releases/v0.1.0-alpha.md)
for capabilities, prototype interface changes and limitations. Draft preparation
does not change the physical milestone status below or authorize publication.

## Milestone status

| Milestone | Status | Evidence still required |
|---|---|---|
| M0 baseline and CI | Software complete | Keep security regressions and native required checks passing on each candidate |
| M1 x86 proof of concept | Software complete | Maintain the demonstrated VM gate on future candidates |
| M2 real mesh | Software implemented; physical gate open | Stock-app QR import, verified USB readback and community/private RF text and ACKs without WAN |
| M3 event kit | Software components implemented; acceptance open | M2 acceptance, fresh x86 install using the runbook, commissioning time and physical recovery/resilience |
| M4 ARM qualification | ARM artifacts realized; physical gate open | M3 acceptance and Pi 4 with 4 GB RAM commissioning/resilience |
| M5 field pilot | Open | M3/M4 acceptance, eight-hour rehearsal and measured resource/RF results |

## Next work in dependency order

1. Qualify the x86 radio path on named hardware. Record Meshtastic app and
   firmware versions, community/private QR scope, USB provisioning/readback,
   channel isolation, text/ACK exchange and disconnect/reconnect without WAN.
2. Validate the fresh x86 event kit. Follow commissioning from installation
   media, measure against the 30-minute target, then exercise reboot, power
   interruption, backup/spare recovery, rollback and direct radio communication
   while the coordinator is stopped. Correct the runbook from observations.
3. Qualify Pi 4 after the x86 kit passes. The current profile imports the generic
   aarch64 SD-image module and disables redistributable firmware. Verify the
   resulting boot/firmware, console or SSH access, Ethernet and USB on Pi 4;
   those settings are qualification questions, not evidence of a known
   regression. Complete the same commissioning and resilience checks on 4 GB
   hardware.
4. Run the eight-hour pilot after both kit gates pass. Measure delivery latency,
   unacknowledged alerts, congestion, memory, storage growth, power and operator
   intervention. Use the [acceptance record](acceptance.md) for each candidate.

Automatic queue admission has a separate repository configuration dependency:
the last recorded check reported `MISH_QUEUE_APP_ID` and
`MISH_QUEUE_APP_PRIVATE_KEY` absent. Authorized CLI admission has exercised the
native merge queue without bypassing required checks. Provision and verify
the repository-scoped GitHub App to complete automatic admission; this does
not block radio qualification or change appliance runtime behavior.

Release workflows create drafts and retain versioned artifacts and checksums.
Physical acceptance remains open, so no release is qualified for event use or
publication. Under 1 GB application RAM, 30-minute commissioning and RF
performance remain targets to measure. Matrix, OpenZiti, Reticulum messaging,
replication, automatic failover and additional board qualification remain
deferred.
