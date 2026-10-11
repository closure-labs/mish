# Live event operator runbook

Use one active coordinator. Keep the private staff radio channel available for
direct communication if the appliance is unavailable. Urgent operational
alerts complement the event's established emergency procedures.

## Before staff arrive

- Confirm WAN is disconnected and the external Ethernet AP provides local DNS.
- Open `https://mish.home.arpa/staff` with a staff credential and `/public`
  without one; verify HTTPS with the installed local CA.
- Confirm the public page shares only the community QR and that staff login
  selects Staff. Explicitly select the intended private department before
  writing department messages. Distribute private QRs only to their members.
- Check authenticated `/api/status` for the selected adapter and connection state.
  Journal author names identify enrolled staff; delivery timestamps and human
  ACK records remain separate.
- Check `systemctl status mish-control caddy` and
  `journalctl -u mish-control -n 100` at the console.
- Send a browser message and a radio message; verify both directions and a
  human `ACK <ID>` on an active alert.
- Check power, USB cable, `/dev/serial/by-id` path, radio channel and free disk
  space. Keep the backup, previous software generation and console available.
- Confirm staff know the accountable responder, expiry and direct-radio fallback.

## Alerts and uncertain delivery

Durable acceptance means the coordinator committed a record. Queue state,
radio submission, transport confirmation and human acknowledgement are
separate. A transport confirmation does not prove anyone read the message.

Check the alert's ID, responder and expiry in the staff journal. Follow up
with the named responder when an active alert lacks explicit acknowledgement.
Browser ACKs identify enrolled staff; radio-reported ACKs retain channel/node
provenance. Neither means every staff member responded. An expired alert
cannot acquire a new ACK; create a fresh alert with a new ID if the issue still
needs attention, making that replacement clear to the responder.

A record marked `uncertain` may already have reached the radio network.
Check with the intended responder over the direct staff channel before creating
a replacement. Uncertain records are never automatically retried and there is
no retry command. Record replacement IDs so staff can distinguish the original.
If the outbox is full, the new message was not accepted: resolve the outage and
retry deliberately, or use direct radio communication.

## Outages

If browsers cannot connect, inspect AP power, local DNS and Ethernet first.
Verify the service at the console and compare the local HTTPS hostname with its
certificate. An internet connectivity warning on a staff device does not by
itself indicate a local service failure. Reconnect devices to the event AP and
re-enter their tokens after reloading the app.

For USB loss, verify cable/power and the configured stable device path. The
adapter reconnects with backoff. Real-radio sends are paced at 30 seconds by
default (`radio.meshtastic.sendIntervalSeconds`); pacing is not a guarantee of
delivery latency. Inspect logs and watch queued records resume;
review any uncertain records independently. Do not change staff channel keys
mid-event without a coordinated radio update. Stop the control service before
external radio edits; reconnect verifies registered keys, slots and RF settings
and refuses changed configurations. Reconnect does not reconfigure the radio.

For service failure, inspect logs and storage before restarting with
`systemctl restart mish-control`. Confirm journal and ACK history after
restart. When the coordinator is unavailable, staff communicate directly by
radio. Keep the old coordinator stopped before activating a restored spare;
never operate two authorities against the same event.

For exposed credentials, run privileged
`mish --config /etc/mish/config.json revoke --id <identity-id>` as the
`mish` service user and enroll a replacement. Enroll a replacement
operator before revoking the last operator.
Revoking browser access does not remove a copied Meshtastic key. Rotate affected
private radio keys and re-enroll the remaining authorized radios separately.

## Recovery and event close

Use the [commissioning and recovery procedure](commissioning.md) for consistent
SQLite backups, CA/config backups, restore and generation rollback. A software
rollback does not roll back event data. Before swapping databases, stop the
application and preserve the existing state; check credentials, scoped journal,
channel membership, ACK history and radio operation before accepting traffic.
Restore resets radio verification; deploy and verify the attached radio before
exporting enrollment or resuming native transmission.
Use `mish restore --input <backup>` into fresh state rather than replacing a
database by hand. Snapshot queue entries become uncertain because they may
have been transmitted after backup capture; review them before creating any
replacement. Normal service restarts preserve the current pending queue.

Retention defaults to seven days and runs hourly; privileged `prune` applies
it immediately. Export the required event records before an explicit wipe.
Stop the service, use `wipe --confirm WIPE-EVENT` as documented, then review
separate credentials, CA material, radio keys, staff devices and removable
backups. Wipe removes state contents and retains the directory for reuse; it
is logical deletion, not forensic erasure.

Record every intervention, duration, affected IDs and outcome in the
[acceptance record](acceptance.md). The rehearsal measures latency, congestion,
memory, storage, power and operator effort; report actual results and unresolved
limitations before publishing a release.
