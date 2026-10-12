# Community rooms

Guest chat is disabled on installation and upgrades. Commission channels as
usual, open `/staff` over trusted HTTPS, and use Community management to enable
enrollment and create rooms. Attendees visit `/public`, choose a nickname, and
join a room. Radio enrollment remains on that page. Staff can enter their bearer
token in the staff access panel to use private mapped rooms.

Wi-Fi rooms do not enqueue radio traffic. Mapped rooms require a verified channel;
public mappings require a community channel. Private mappings use current staff
channel authorization on every request. Mapping and visibility are immutable;
archive and recreate to change either. Only one active room maps each channel.
Earlier journal history is never imported into a newly created room.

Nicknames are display names with an eight-hex identity suffix. Sessions contain
256 random bits, are stored as SHA-256 hashes separately from staff, expire after
24 hours, and use Secure, HttpOnly, SameSite=Strict host-only cookies. Revoking a
guest invalidates their sessions but does not prevent another enrollment. Guest
writes require an exact HTTPS Origin/Host match. Deploy behind the supplied Caddy
proxy; do not expose the loopback API directly to attendee networks.

Text is limited to 160 UTF-8 bytes. Radio submissions include the author label in
that limit. Accepted messages are durable before responses return. A client key
is unique per identity; reuse with the same room/text returns the original
acceptance, and reuse with different content fails. Acceptance deduplication is
retained for the configured journal retention period. Drafts and pending keys
are saved on that browser. Reconnection never automatically submits a draft.

Default limits: 120 enrollments/hour appliance-wide, 30 posts/minute/identity,
and 10 queued guest radio messages. Operators can change these through the UI.
Guest traffic cannot consume the final half of the radio queue; pending staff
traffic sorts first. Existing radio pacing and uncertain-send behavior apply.
Wi-Fi capacity is independent of radio capacity. Hide removes text from room
projections; it cannot retract a radio transmission or copies already read.

The API uses `/api/community/session`, `/rooms`, `/rooms/{id}/membership` and
`/rooms/{id}/messages?after=0` beneath `/api/community`. Message POST bodies are
`{"key":"client-generated-id","text":"hello"}`. Pagination returns at most 100
entries, an advancing cursor, paginated hidden cursors, and recent radio delivery updates. Pass the returned
`moderation_cursor` as `moderation_after` alongside the message cursor.
The browser polls every two seconds while visible, backing off to 30 seconds.
Operator bearer requests to `/api/community/manage` accept tagged actions
`configure`, `create`, `archive`, `revoke`, `hide`, and `list`; see `Manage` in
`apps/mish/src/community.rs` for the validated fields.

All room tables live in `events.sqlite`: backup, restore and wipe include them.
Retention removes entries, moderation, idempotency results, and expired guest
sessions, then unused old guests and their memberships. Room definitions remain.
Restore quarantines radio work and requires channel verification as before.

## Qualification

Rust integration tests cover 50 enrolled identities and 500 local messages,
restart/backup restore, idempotency conflicts, membership, revocation, guest/staff
separation, origin/cookie protections, moderation, private room invisibility,
radio limits/priority, stale bindings, and receive deduplication without echo.
This is not the one-hour concurrent browser qualification. Native CI package/VM
checks and physical iOS Safari/Android Chrome, WAN-disconnected operation,
trusted event DNS/TLS and USB reconnect remain separately recorded gates.

The `Community load qualification` PR workflow runs 50 real HTTP clients for one hour,
posts one aggregate message per second, asserts every client sees every accepted
message within five seconds, and uploads revision-tagged logs. It is an HTTP
load qualification, not browser rendering or hardware acceptance. Locally use
`bash scripts/local-nix.sh --command cargo test --manifest-path apps/mish/Cargo.toml --test http community_http_soak -- --ignored --exact --nocapture`.
`MISH_SOAK_SECONDS=10` shortens a smoke run; it must not be reported as the hour
qualification. The local 10-second run passed with all 50 clients receiving all
10 messages within the five-second bound.

The workflow also runs 50 isolated Chromium browser sessions for one hour,
checks rendered ordered text and a five-second visibility bound at one aggregate
send per second. `tests/community-browser.cjs` uses ephemeral test TLS with a
browser trust bypass; it cannot qualify event certificate trust or phone
platforms. Set `MISH_BROWSER_TOOLS` to the pinned Chromium/playwright-driver
symlinkJoin built by the workflow. Local smoke settings are
`MISH_BROWSER_CLIENTS=5 MISH_BROWSER_SECONDS=10`; the 50-browser run exceeded
the local 2 GB launcher limit and belongs on native CI.
