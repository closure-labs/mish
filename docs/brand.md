# Writing about Mish

Use **Mish** for the product name and `mish` for commands, package names,
configuration identifiers and paths. Keep operator instructions concrete:
name the action, its result and the evidence needed to verify it.

## Positioning

Mish is an offline, owner-operated event coordination appliance. Its audience
is event operators and staff who need local text communication, accountable
operational alerts and official attendee announcements.

Lead with the work people can do: coordinate privately, acknowledge an alert,
publish an announcement and recover event history. Explain the local AP,
coordinator and radio topology when it helps someone prepare their kit.
Implementation detail belongs in the API, architecture and build guides.

## Claims and evidence

| Say | Evidence or qualification |
|---|---|
| Local event operation needs no hosted account or WAN | The deployed coordinator, AP and radio path are local; preparation may download build dependencies |
| Staff records remain private until an operator publishes chosen announcement text | Staff/operator authorization and a separate public projection |
| Alerts have explicit human acknowledgements | Distinguish enrolled browser ACKs from radio-reported node/channel ACKs |
| Writes receive durable local acceptance | Distinguish that acceptance from RF delivery and human reading |
| Software checks and images passed | Name the tested revision and link the verification record |
| Hardware is ready for an event | Requires completed physical commissioning, USB/RF, resilience and rehearsal evidence |

Keep engineering targets labelled as targets and pilot measurements labelled
as measurements. Preserve the distinction between automatic reconnection,
uncertain delivery and operator-reviewed replacement. Describe optional
integrations according to their implemented capability and acceptance scope.

Use [README.md](../README.md) for the public introduction,
[roadmap.md](roadmap.md) for decisions and gates, and
[verification.md](verification.md) for revision-specific results.
