from __future__ import annotations

import tempfile
import time
import unittest
from unittest.mock import patch

from mish.appliance import EventStore
from mish.radio import RadioWorker, SimulatorTransport


class RadioTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.store = EventStore(self.directory.name)
        self.config = {"radio": {"adapter": "simulator", "meshtastic": {"channel": 2}}}

    def tearDown(self):
        self.directory.cleanup()

    def test_simulator_is_explicit_and_submission_is_not_ack(self):
        record = self.store.create("Staff meet at gate", "staff-a")
        disabled = RadioWorker(self.store, {})
        disabled.tick(0)
        self.assertEqual(self.store.record(record["id"])["state"], "queued")
        worker = RadioWorker(self.store, self.config)
        worker.tick(0)
        persisted = self.store.record(record["id"])
        self.assertEqual(persisted["state"], "submitted")
        self.assertEqual(persisted["acknowledgements"], [])
        self.assertEqual(worker.transport.sent[0]["channel"], 2)
        self.assertEqual(SimulatorTransport("", None).send("x", 2, record["id"]), persisted["packet_id"])

    def test_ambiguous_send_is_not_retried_after_restart(self):
        record = self.store.create("Please check gate", "staff-a")

        class Ambiguous(SimulatorTransport):
            def send(self, text, channel, record_id):
                super().send(text, channel, record_id)
                raise ConnectionError("USB lost after write")

        worker = RadioWorker(self.store, self.config, Ambiguous)
        with self.assertLogs("mish.radio", level="WARNING"):
            worker.tick(0)
        self.assertEqual(self.store.record(record["id"])["state"], "uncertain")
        restarted = RadioWorker(EventStore(self.directory.name), self.config)
        restarted.tick(10)
        self.assertEqual(restarted.transport.sent, [])

    def test_crash_during_send_recovers_as_uncertain(self):
        record = self.store.create("Please check gate", "staff-a")
        self.store.submission(record["id"], "sending")
        reopened = EventStore(self.directory.name)
        self.assertEqual(reopened.record(record["id"])["state"], "uncertain")
        self.assertEqual(reopened.pending(), [])

    def test_connection_backoff_preserves_queue_then_reconnects(self):
        record = self.store.create("Checking gate", "staff-a")
        attempts = []

        def factory(device, callback):
            attempts.append(device)
            if len(attempts) < 3:
                raise ConnectionError("USB unplugged")
            return SimulatorTransport(device, callback)

        worker = RadioWorker(self.store, self.config, factory)
        with self.assertLogs("mish.radio", level="WARNING"):
            worker.tick(0)
        worker.tick(0.5)
        self.assertEqual(len(attempts), 1)
        with self.assertLogs("mish.radio", level="WARNING"):
            worker.tick(1)
        worker.tick(2.9)
        self.assertEqual(len(attempts), 2)
        self.assertEqual(self.store.record(record["id"])["state"], "queued")
        worker.tick(3)
        self.assertEqual(self.store.record(record["id"])["state"], "submitted")
        worker.transport.inject({"id": 42, "fromId": "!radio", "channel": 2,
                                 "decoded": {"text": "radio message"}})
        worker._callback("disconnect", None)
        worker.tick(4)
        self.assertIsNone(worker.transport)
        worker.tick(5)
        self.assertIsNotNone(worker.transport)

    def test_radio_ack_retains_provenance_deduplicates_and_never_requeues(self):
        alert = self.store.create("Gate needs a responder", "staff-a", kind="urgent",
                                  responder="staff-b", expires=time.time() + 600)
        worker = RadioWorker(self.store, self.config)
        worker.tick(0)
        self.assertIn(alert["id"], worker.transport.sent[0]["text"])
        packet = {"id": 42, "fromId": "!1234", "channel": 2,
                  "decoded": {"text": "ACK " + alert["id"]}}
        worker.transport.inject(packet)
        worker.transport.inject(packet)
        worker.tick(1)
        stored = self.store.record(alert["id"])
        self.assertEqual(len(stored["acknowledgements"]), 1)
        ack = stored["acknowledgements"][0]
        self.assertEqual(ack["node"], "!1234")
        self.assertEqual(ack["channel"], 2)
        self.assertEqual(ack["source"], "radio")
        self.assertEqual(stored["state"], "submitted")
        self.assertEqual(len(worker.transport.sent), 1)
        self.assertEqual(self.store.pending(), [])

    def test_transport_confirmation_is_distinct_and_nak_is_not_success(self):
        record = self.store.create("Checking gate", "staff-a")
        worker = RadioWorker(self.store, self.config)
        worker.tick(0)
        packet_id = self.store.record(record["id"])["packet_id"]
        worker.transport.inject({"decoded": {"requestId": packet_id,
                                              "routing": {"errorReason": "NO_ROUTE"}}})
        worker.tick(1)
        self.assertEqual(self.store.record(record["id"])["state"], "submitted")
        worker.transport.inject({"decoded": {"requestId": packet_id, "routing": {}}})
        worker.tick(2)
        stored = self.store.record(record["id"])
        self.assertEqual(stored["state"], "confirmed")
        self.assertEqual(stored["acknowledgements"], [])

    def test_other_channel_and_malformed_packets_do_not_enter_journal(self):
        worker = RadioWorker(self.store, self.config)
        worker.tick(0)
        for packet in (None, {"decoded": []},
                       {"id": 1, "from": 2, "channel": 0, "decoded": {"text": "private"}},
                       {"id": 2, "from": 2, "channel": 2, "decoded": {"text": "x" * 201}}):
            worker.transport.inject(packet)
        worker.tick(1)
        self.assertEqual(self.store.records(), [])

    def test_expired_alert_is_not_sent(self):
        alert = self.store.create("Expired alert", "staff-a", kind="alert",
                                  responder="staff-b", expires=time.time() + 100)
        with self.store.connect() as db:
            db.execute("UPDATE records SET expires=? WHERE id=?", (time.time() - 1, alert["id"]))
        worker = RadioWorker(self.store, self.config)
        worker.tick(0)
        self.assertEqual(worker.transport.sent, [])
        self.assertEqual(self.store.record(alert["id"])["state"], "expired")

    def test_real_adapter_paces_queue_and_keeps_callbacks_active(self):
        first = self.store.create("First", "staff-a")
        alert = self.store.create("Expires while waiting", "staff-a", kind="alert",
                                  responder="lead", expires=time.time() + 100)
        third = self.store.create("Third", "staff-a")
        config = {"radio": {"adapter": "meshtastic", "meshtastic": {"channel": 2}}}
        worker = RadioWorker(self.store, config, SimulatorTransport)
        worker.tick(0)
        self.assertEqual(len(worker.transport.sent), 1)
        self.assertEqual(self.store.record(first["id"])["state"], "submitted")
        self.assertEqual(self.store.record(third["id"])["state"], "queued")
        packet_id = worker.transport.sent[0]["id"]
        worker.transport.inject({"decoded": {"requestId": packet_id, "routing": {}}})
        worker.tick(1)
        self.assertEqual(self.store.record(first["id"])["state"], "confirmed")
        self.assertEqual(len(worker.transport.sent), 1)
        with self.store.connect() as db:
            db.execute("UPDATE records SET expires=? WHERE id=?", (time.time() - 1, alert["id"]))
        worker.tick(29.9)
        self.assertEqual(self.store.record(alert["id"])["state"], "expired")
        worker.tick(30)
        self.assertEqual(len(worker.transport.sent), 2)
        self.assertEqual(self.store.record(third["id"])["state"], "submitted")
        self.assertFalse(any(alert["id"] in item["text"] for item in worker.transport.sent))

    def test_alert_expiring_after_pending_snapshot_is_not_sent(self):
        alert = self.store.create("Almost expired", "staff-a", kind="urgent",
                                  responder="lead", expires=time.time() + 100)
        expires = self.store.record(alert["id"])["expires"]
        snapshot = self.store.pending()
        worker = RadioWorker(self.store, self.config)
        with patch.object(self.store, "pending", return_value=snapshot), patch("mish.radio.time.time", return_value=expires + 1):
            worker.tick(0)
        self.assertEqual(worker.transport.sent, [])
        self.assertEqual(self.store.record(alert["id"])["state"], "expired")

    def test_real_adapter_rejects_invalid_pacing(self):
        for value in (0, -1, True, "30", float("inf"), float("nan")):
            config = {"radio": {"adapter": "meshtastic", "meshtastic": {"sendIntervalSeconds": value}}}
            with self.subTest(value=value), self.assertRaises(ValueError):
                RadioWorker(self.store, config, SimulatorTransport)

    def test_worker_does_not_send_record_when_durable_claim_is_rejected(self):
        record = self.store.create("Claimed by another worker", "staff-a")
        snapshot = self.store.pending()
        self.store.submission(record["id"], "sending")
        worker = RadioWorker(self.store, self.config)
        with patch.object(self.store, "pending", return_value=snapshot):
            worker.tick(0)
        self.assertEqual(worker.transport.sent, [])
        self.assertEqual(self.store.record(record["id"])["state"], "sending")

    def test_full_rf_payload_range_and_deduplication_retention(self):
        worker = RadioWorker(self.store, self.config)
        worker.tick(0)
        for size in (161, 200):
            packet = {"id": size, "fromId": "!1234", "channel": 2,
                      "decoded": {"text": "x" * size}}
            worker.transport.inject(packet)
            worker.transport.inject(packet)
        worker.tick(1)
        self.assertEqual(sorted(len(item["text"]) for item in self.store.records()), [161, 200])
        with self.store.connect() as db:
            self.assertEqual(db.execute("SELECT count(*) FROM received").fetchone()[0], 2)
            db.execute("UPDATE received SET created=0")
        self.store.prune(7)
        with self.store.connect() as db:
            self.assertEqual(db.execute("SELECT count(*) FROM received").fetchone()[0], 0)


if __name__ == "__main__":
    unittest.main()
