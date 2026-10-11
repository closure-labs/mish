# Writing about Mish

Use **Mish** for the product name and `mish` for commands, package names,
configuration identifiers and paths. Keep operator instructions concrete:
name the action, its result and the evidence needed to verify it.

## Positioning

Mish is an offline, owner-operated event coordination appliance. Its audience
is event operators, staff and community participants who need a shared local
mesh alongside private staff coordination and accountable operational alerts.

Lead with the work people can do: join the community mesh, coordinate privately,
acknowledge an alert and recover event history. Explain public enrollment and
private staff/department QR scope, then the local AP,
coordinator and radio topology when it helps someone prepare their kit.
Implementation detail belongs in the API, architecture and build guides.

## Claims and evidence

| Say | Evidence or qualification |
|---|---|
| Local event operation needs no hosted account or WAN | The deployed coordinator, AP and radio path are local; preparation may download build dependencies |
| A public QR lets people participate on the community radio channel | It intentionally shares the community PSK and permits transmission |
| Private staff and department enrollment is scoped | Explicit web membership and exports that include only community plus the selected authorized private channel |
| Radio enrollment and web identity are separate | A copied PSK is not revoked by withdrawing a browser token; RF removal needs key rotation and radio re-enrollment |
| Alerts have explicit human acknowledgements | Distinguish enrolled browser ACKs from radio-reported node/channel ACKs |
| Writes receive durable local acceptance | Distinguish that acceptance from RF delivery and human reading |
| Software checks and images passed | Name the tested revision and link the verification record |
| Hardware is ready for an event | Requires completed physical commissioning, USB/RF, resilience and rehearsal evidence |

Channel generation and provisioning claims require their own implementation
and test evidence. Earlier application checks do not validate a new QR or
hardware deployment path. Keep engineering targets labelled as targets and pilot measurements labelled
as measurements. Preserve the distinction between automatic reconnection,
uncertain delivery and operator-reviewed replacement. Describe optional
integrations according to their implemented capability and acceptance scope.

Use [README.md](../README.md) for the public introduction,
[roadmap.md](roadmap.md) for decisions and gates, and
[verification.md](verification.md) for revision-specific results.
