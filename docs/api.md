# Mish HTTP API

The bundled staff application uses this API. The service binds to loopback by
default, with Caddy exposing local HTTPS on the appliance. Use
`https://mish.home.arpa` after [commissioning](commissioning.md), including trust
of the event's local certificate authority.

The Rust service implements HTTP with Axum and the owner CLI with Clap. A
bounded Tokio coordinator sends storage commands to a dedicated SQLite thread;
accepted writes return after transaction commit. The wire routes and CLI names
remain the event application's existing contract. See
[rust-architecture.md](rust-architecture.md) for implementation boundaries.

Send `Authorization: Bearer <token>` for authenticated operations. Staff tokens
permit private event reads and staff writes. Operator tokens also permit
enrollment, revocation and channel membership. Enrollment returns a token once;
revocation prevents that token from authenticating. The last operator cannot
be revoked until another operator is enrolled.

## Routes

Channel enrollment and scoped-message routes below describe the implemented
Rust contract. Physical interoperability remains a separate acceptance gate.

| Route | Access | Request / result |
|---|---|---|
| `GET /health` | Public | Minimal coordinator health |
| `GET /` or `/staff` | Public app shell | Staff HTML; data still requires a token |
| `GET /public` | Public | Community enrollment landing page; no anonymous browser message writes |
| `GET /public/channels` | Public | Community-only channel catalog |
| `GET /public/channels/{id}/enrollment` | Public | Community URL and `qr_svg`; private channel export is forbidden |
| `GET /api/channels` | Staff | Channels permitted by the authenticated identity's membership |
| `GET /api/channels/{id}/enrollment` | Staff | Enrollment scoped to an authorized channel |
| `POST /api/channels/{id}/access` | Operator | `staff_id`, `grant` boolean; change channel membership |
| `GET /api/status` | Staff | Selected adapter and radio connection state |
| `GET /api/messages` | Staff | Journal filtered by channel membership, author names, delivery state and ACK history |
| `POST /api/messages` | Staff | `text`, optional `channel` (default `staff`); author comes from the authenticated identity |
| `POST /api/alerts` | Staff | `text`, `responder`, `expires`, optional `kind` (`alert` or `urgent`) and `channel` (default `staff`) |
| `POST /api/alerts/{id}/ack` | Staff | Explicit acknowledgement by the authenticated identity |
| `POST /api/staff` | Operator | `name` and optional `role` (`staff` or `operator`); returns enrollment token |
| `DELETE /api/staff/{id}` | Operator | Revoke the identity's token |

Requests containing JSON must be objects with `Content-Type: application/json`;
typed event requests reject unknown fields.
The body limit is 16 KiB. Messages and alert text are nonempty and limited to
160 UTF-8 bytes. Alert expiration is a Unix epoch timestamp within the next
24 hours. The browser currently uses a one-hour expiration.
Staff names are limited to 80 UTF-8 bytes and announcements to 4,000 UTF-8 bytes.

For example, an authenticated message request body is:

```json
{"text": "Water team, please check in at the north gate", "channel": "staff"}
```

Successful creation returns HTTP 201 after durable acceptance. Invalid input
returns 400, absent or invalid credentials return 401, and a staff credential
used for an operator action returns 403. A full outbound queue rejects new
acceptance rather than claiming success. Oversized bodies return 413;
unavailable coordinator/storage operations return 503 without claiming
acceptance. The public community landing page exposes radio enrollment; it
does not grant anonymous browser messaging or staff access.

## Delivery and privacy

The journal keeps durable local acceptance, current queue state, radio
submission, transport confirmation and transmission uncertainty distinct.
Persisted delivery timestamps survive restart. Human acknowledgements are
separate records; transport confirmation does not imply receipt or reading by
staff. Expired alerts cannot receive new acknowledgements.

Radio replies `ACK <ID>` are recorded with their node/channel provenance and
shown as radio-reported acknowledgements. Browser acknowledgements identify
enrolled staff. Neither establishes that all recipients responded.

Channel membership filters staff journal reads, message writes, alerts and
private enrollment export. `GET /api/messages?channel=department-medical`
selects one authorized channel. Without a selection, private rows take priority
over community rows within the 500-record response limit. Public catalog/enrollment responses expose only
community configuration and its intentionally shared key, never staff or
department keys or identities. A community QR permits radio transmission;
its holder does not gain staff browser access. See
[channel-enrollment.md](channel-enrollment.md) for QR scope, explicit provisioning
and rotation, and the [operator runbook](operator-runbook.md) for
uncertain sends and expired-alert handling.

Revoking a web token or channel membership cannot revoke a radio key already
copied to another device. Rotate the affected channel key and re-enroll the
remaining authorized radios to remove that RF access.

## Deprecated announcement compatibility

`GET /public/announcements` and operator `POST /api/announcements` are retained
for compatibility with earlier revisions. They use a separate public text
projection and do not publish private source objects automatically. They are
not the new community mesh's enrollment or access model. The public product
surface is participatory Meshtastic channel enrollment.

Real-radio sends are paced by
`services.mish.radio.meshtastic.sendIntervalSeconds`, with a 30-second default.
That interval controls submission pacing; it is not an RF delivery promise.

## Development interfaces

`GET /waypoints` and `GET /geopacks` require operator authorization, as do
`POST /waypoints/simulated` and `POST /geopacks`. Simulated waypoint creation
also requires the simulator to be enabled. GeoPack publication remains a local
CLI operation during the prototype. These facilities are outside the event
communication acceptance path.

The local `mish` CLI is a privileged owner interface. It manages commissioning,
enrollment, revocation, messages, alerts, acknowledgements, publication,
backup/restore, retention and wipe. Use the [commissioning guide](commissioning.md)
for its commands and recovery safeguards.
