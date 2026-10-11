"""Durable, conservative radio submission; RF confirmation is not a human ACK.

Meshtastic API reference: https://python.meshtastic.org/mesh_interface.html
No received packet is transmitted automatically. A crash between the durable
``sending`` transition and recording the packet ID must recover as uncertain.
"""

from __future__ import annotations

import hashlib
import logging
import math
import queue
import threading
import time
from typing import Any

LOG = logging.getLogger(__name__)
MAX_TEXT_BYTES = 200


class SimulatorTransport:
    """Explicit simulation: deterministic submissions, no imaginary recipients."""

    def __init__(self, device: str, callback: Any):
        self.callback = callback
        self.sent: list[dict[str, Any]] = []

    def send(self, text: str, channel: int, record_id: str) -> str:
        packet_id = "sim-" + hashlib.sha256(record_id.encode()).hexdigest()[:16]
        self.sent.append({"id": packet_id, "text": text, "channel": channel})
        # A simulator submission does not manufacture transport or human ACKs.
        return packet_id

    def inject(self, packet: dict[str, Any]) -> None:
        self.callback("packet", packet)

    def close(self) -> None:
        pass


class MeshtasticTransport:
    """Optional Python Meshtastic serial API, scoped to one explicit USB device."""

    def __init__(self, device: str, callback: Any):
        if not device.startswith("/dev/serial/by-id/"):
            raise ValueError("radio device must use /dev/serial/by-id/")
        from meshtastic.serial_interface import SerialInterface
        from pubsub import pub

        self.callback = callback
        self.pub = pub
        self.interface = None
        self.connected = True
        pub.subscribe(self._receive, "meshtastic.receive")
        pub.subscribe(self._lost, "meshtastic.connection.lost")
        try:
            self.interface = SerialInterface(devPath=device)
        except Exception:
            pub.unsubscribe(self._receive, "meshtastic.receive")
            pub.unsubscribe(self._lost, "meshtastic.connection.lost")
            raise

    def _receive(self, packet: dict[str, Any], interface: Any) -> None:
        if interface is self.interface:
            self.callback("packet", packet)

    def _lost(self, interface: Any) -> None:
        if interface is self.interface:
            self.connected = False
            self.callback("disconnect", None)

    def send(self, text: str, channel: int, record_id: str) -> str:
        if not self.connected:
            raise ConnectionError("radio disconnected")
        packet = self.interface.sendText(text, channelIndex=channel, wantAck=True)
        if not packet.id:
            raise RuntimeError("radio submission returned no packet ID")
        return str(packet.id)

    def close(self) -> None:
        self.pub.unsubscribe(self._receive, "meshtastic.receive")
        self.pub.unsubscribe(self._lost, "meshtastic.connection.lost")
        if self.interface is not None:
            self.interface.close()


class RadioWorker:
    """One writer drains the durable outbox; callbacks enqueue for that writer."""

    def __init__(self, store: Any, config: dict[str, Any], transport_factory: Any = None):
        self.store = store
        radio = config.get("radio", {})
        settings = radio.get("meshtastic", {})
        self.adapter = radio.get("adapter", "meshtastic" if settings.get("enable") else "disabled")
        self.device = settings.get("device", "")
        self.channel = settings.get("channel", 0)
        if type(self.channel) is not int or not 0 <= self.channel <= 7:
            raise ValueError("radio channel must be an integer from 0 to 7")
        if self.adapter not in {"disabled", "simulator", "meshtastic"}:
            raise ValueError("unknown radio adapter")
        self.send_interval = settings.get("sendIntervalSeconds", 30 if self.adapter == "meshtastic" else 0)
        if (
            type(self.send_interval) not in {int, float}
            or not math.isfinite(self.send_interval)
            or self.send_interval < 0
            or (self.adapter == "meshtastic" and self.send_interval == 0)
        ):
            raise ValueError("radio send interval must be finite and positive (simulator permits zero)")
        self.next_send_at = 0.0
        self.factory = transport_factory or (
            SimulatorTransport if self.adapter == "simulator" else MeshtasticTransport
        )
        self.transport: Any = None
        self.events: queue.Queue = queue.Queue(maxsize=1024)
        self.retry_at = 0.0
        self.backoff = 1.0
        self.stopped = threading.Event()
        self.thread: threading.Thread | None = None
        self.status = "disabled" if self.adapter == "disabled" else "disconnected"

    def _callback(self, kind: str, value: Any) -> None:
        try:
            self.events.put_nowait((kind, value))
        except queue.Full:
            # Never invent delivery success under overload. Hardware may redeliver.
            LOG.warning("radio receive queue exhausted; packet was dropped")

    def _disconnect(self, now: float) -> None:
        if self.transport is not None:
            try:
                self.transport.close()
            except Exception:
                LOG.warning("radio close failed", exc_info=True)
        self.transport = None
        self.status = "disconnected"
        self.retry_at = now + self.backoff
        self.backoff = min(60.0, self.backoff * 2)

    def _packet(self, packet: dict[str, Any]) -> None:
        if not isinstance(packet, dict):
            return
        decoded = packet.get("decoded", {})
        if not isinstance(decoded, dict):
            return
        routing = decoded.get("routing")
        request_id = decoded.get("requestId")
        if isinstance(routing, dict) and request_id:
            if routing.get("errorReason", "NONE") == "NONE":
                self.store.transport_confirmation(str(request_id))
            # Negative routing replies must never turn into human acknowledgement.
            return
        channel = packet.get("channel", 0)
        node = packet.get("fromId", packet.get("from"))
        packet_id = packet.get("id")
        text = decoded.get("text")
        if (
            channel != self.channel
            or type(channel) is not int
            or node is None
            or not packet_id
            or not isinstance(text, str)
            or not text.strip()
            or len(text.encode("utf-8")) > MAX_TEXT_BYTES
        ):
            return
        self.store.receive(str(packet_id), str(node), channel, text)

    def tick(self, now: float | None = None) -> None:
        """One polling step; monotonic time can be supplied by deterministic tests."""
        now = time.monotonic() if now is None else now
        if self.adapter == "disabled":
            return
        while True:
            try:
                kind, value = self.events.get_nowait()
            except queue.Empty:
                break
            if kind == "disconnect":
                self._disconnect(now)
            elif kind == "packet":
                try:
                    self._packet(value)
                except (ValueError, KeyError, TypeError):
                    LOG.warning("ignored malformed radio packet")
        if self.transport is None:
            if now < self.retry_at:
                return
            try:
                self.transport = self.factory(self.device, self._callback)
                self.status = "simulated" if self.adapter == "simulator" else "connected"
            except Exception:
                LOG.warning("radio connection failed", exc_info=True)
                self._disconnect(now)
                return
        for record in self.store.pending():
            if self.stopped.is_set():
                return
            # Pending records stay on disk while the RF airtime budget is paced.
            # Callbacks above still run during this wait, including human ACKs.
            if now < self.next_send_at:
                return
            if record.get("expires") is not None and record["expires"] <= time.time():
                self.store.submission(record["id"], "expired")
                continue
            text = record["text"]
            if record["kind"] in {"alert", "urgent"}:
                text = f"{record['kind'].upper()} {record.get('short_id', record['id'])}: {text}"
            if len(text.encode("utf-8")) > MAX_TEXT_BYTES:
                self.store.submission(record["id"], "failed")
                continue
            # Commit before calling the radio: restart cannot retry an ambiguous send.
            if self.store.submission(record["id"], "sending") is False:
                continue
            self.next_send_at = now + self.send_interval
            try:
                packet_id = self.transport.send(text, self.channel, record["id"])
            except Exception:
                self.store.submission(record["id"], "uncertain")
                LOG.warning("radio submission uncertain; automatic retry suppressed", exc_info=True)
                self._disconnect(now)
                return
            self.store.submission(record["id"], "submitted", str(packet_id))
            self.backoff = 1.0

    def start(self) -> None:
        if self.thread is not None or self.adapter == "disabled":
            return
        self.thread = threading.Thread(target=self._run, name="mish-radio", daemon=True)
        self.thread.start()

    def _run(self) -> None:
        try:
            while not self.stopped.is_set():
                self.tick()
                self.stopped.wait(0.5)
        finally:
            if self.transport is not None:
                self.transport.close()

    def stop(self) -> None:
        self.stopped.set()
        if self.thread is not None:
            self.thread.join(timeout=10)
