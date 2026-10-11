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
enrollment, revocation and publication. Enrollment returns a token once;
revocation prevents that token from authenticating. The last operator cannot
be revoked until another operator is enrolled.

## Routes

| Route | Access | Request / result |
|---|---|---|
| `GET /health` | Public | Minimal coordinator health |
| `GET /` or `/staff` | Public app shell | Staff HTML; data still requires a token |
| `GET /public` | Public | Read-only attendee HTML |
| `GET /public/announcements` | Public | Published announcement text and publication metadata |
| `GET /api/status` | Staff | Selected adapter and radio connection state |
| `GET /api/messages` | Staff | Private journal, author names, delivery state and acknowledgement history |
| `POST /api/messages` | Staff | `text`; author comes from the authenticated identity |
| `POST /api/alerts` | Staff | `text`, `responder`, `expires` and optional `kind` (`alert` or `urgent`) |
| `POST /api/alerts/{id}/ack` | Staff | Explicit acknowledgement by the authenticated identity |
| `POST /api/staff` | Operator | `name` and optional `role` (`staff` or `operator`); returns enrollment token |
| `DELETE /api/staff/{id}` | Operator | Revoke the identity's token |
| `POST /api/announcements` | Operator | `text`; creates a separate public announcement |

Requests containing JSON must be objects with `Content-Type: application/json`;
typed event requests reject unknown fields.
The body limit is 16 KiB. Messages and alert text are nonempty and limited to
160 UTF-8 bytes. Alert expiration is a Unix epoch timestamp within the next
24 hours. The browser currently uses a one-hour expiration.
Staff names are limited to 80 UTF-8 bytes and announcements to 4,000 UTF-8 bytes.

For example, an authenticated message request body is:

```json
{"text": "Water team, please check in at the north gate"}
```

Successful creation returns HTTP 201 after durable acceptance. Invalid input
returns 400, absent or invalid credentials return 401, and a staff credential
used for an operator action returns 403. A full outbound queue rejects new
acceptance rather than claiming success. Oversized bodies return 413;
unavailable coordinator/storage operations return 503 without claiming
acceptance. The attendee routes have no write operation.

## Delivery and privacy

The journal keeps durable local acceptance, current queue state, radio
submission, transport confirmation and transmission uncertainty distinct.
Persisted delivery timestamps survive restart. Human acknowledgements are
separate records; transport confirmation does not imply receipt or reading by
staff. Expired alerts cannot receive new acknowledgements.

Radio replies `ACK <ID>` are recorded with their node/channel provenance and
shown as radio-reported acknowledgements. Browser acknowledgements identify
enrolled staff. Neither establishes that all recipients responded.

Publication copies only the operator's chosen announcement text into a separate
public projection. Private messages, staff identities, positions and
unpublished alerts do not become public automatically. Incoming RF posts never
publish themselves. See the [operator runbook](operator-runbook.md) for
uncertain sends and expired-alert handling.

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
