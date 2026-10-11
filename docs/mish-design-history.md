# Mish design history

Mish's earlier design explored a broad communications appliance combining
federated messaging, identity-based networking, signed public publication,
location packs and several radio transports. Those proposals were design
options, not evidence of delivered capabilities.

The current direction narrows the product to an offline, owner-operated event
appliance: private staff text, operational alerts with explicit acknowledgements
and a separately published, read-only attendee feed. One local coordinator owns
the SQLite journal and outbound queue. The application serves bundled browser
assets, with Caddy local HTTPS and a USB Meshtastic adapter.

## Superseded concepts

| Earlier concept | Current decision |
|---|---|
| OpenZiti as the appliance's access fabric | Staff/operator bearer authorization protects the current app; the optional OpenZiti services are health stubs |
| Matrix as the delivered collaboration layer | The bundled local staff app supplies event messaging; Matrix providers remain optional health stubs |
| Nostr as the official announcement path | Operators publish into a separate local attendee projection; existing mock/file Nostr facilities are development tools |
| Multiple radio transports in the first acceptance gate | Qualify one USB Meshtastic path first; Reticulum daemon wiring has no application bridge |
| GeoPacks and location exchange as core event workflows | Retain existing development interfaces while deferring expansion |
| Broad board and home-server release targets | Start with x86_64, then physically qualify Pi 4 with 4 GB RAM; other profiles are experimental |
| A federated or clustered appliance architecture | One authoritative coordinator, with manually activated spares and conservative snapshot recovery |
| A signed application store and general appliance ecosystem | Outside the first event-appliance release |

Mish is the name used for the repository's application, CLI, module, services,
configuration and runtime paths. `mish.home.arpa` is the local hostname used by
the commissioning procedure.

## Evidence-led next steps

The [roadmap](roadmap.md) defines the milestone dependencies and acceptance
gates. The [verification record](verification.md) names the revisions whose
automated checks, VM behavior and image builds passed. Physical commissioning,
USB/RF operation, resilience and the eight-hour rehearsal require the separate
[acceptance record](acceptance.md).

Replication, delay-tolerant forwarding and formal alert-protocol verification
remain possible future tracks. Revisit them after field measurements show a
need; they are not runtime dependencies or current product promises.
