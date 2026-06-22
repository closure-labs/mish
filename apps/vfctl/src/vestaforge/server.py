from __future__ import annotations

import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any
from urllib.parse import urlparse

from .storage import VestaForgeStore, status_payload


class VestaForgeHandler(BaseHTTPRequestHandler):
    store: VestaForgeStore
    config: dict[str, Any]

    def log_message(self, fmt: str, *args: Any) -> None:
        return

    def _read_json(self) -> dict[str, Any]:
        length = int(self.headers.get("Content-Length", "0"))
        if length == 0:
            return {}
        raw = self.rfile.read(length).decode("utf-8")
        return json.loads(raw)

    def _send(self, status: int, payload: Any) -> None:
        body = json.dumps(payload, indent=2, sort_keys=True).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _send_error(self, status: int, message: str) -> None:
        self._send(status, {"ok": False, "error": message})

    def do_GET(self) -> None:  # noqa: N802 - stdlib handler naming.
        parsed = urlparse(self.path)
        if parsed.path == "/health":
            self._send(200, status_payload(self.config, include_probes=False))
            return
        if parsed.path == "/waypoints":
            self._send(200, {"waypoints": self.store.list_waypoints()})
            return
        if parsed.path == "/geopacks":
            self._send(200, {"geopacks": self.store.list_geopacks()})
            return
        self._send_error(404, "not found")

    def do_POST(self) -> None:  # noqa: N802 - stdlib handler naming.
        parsed = urlparse(self.path)
        try:
            if parsed.path == "/waypoints/simulated":
                payload = self._read_json()
                position = payload.get("position", {})
                waypoint, created = self.store.add_simulated_waypoint(
                    lat=payload.get("lat", position.get("lat")),
                    lon=payload.get("lon", position.get("lon")),
                    label=payload.get("label"),
                    body=payload.get("body"),
                    visibility=payload.get("visibility", "private"),
                    waypoint_id=payload.get("id"),
                    source_payload=payload.get("payload", {}),
                )
                self._send(201 if created else 200, {"created": created, "waypoint": waypoint})
                return
            if parsed.path == "/geopacks":
                payload = self._read_json()
                waypoint_ids = payload.get("waypointIds") or payload.get("waypoints") or []
                manifest, created = self.store.create_geopack(
                    name=payload.get("name", ""),
                    waypoint_ids=waypoint_ids,
                    visibility=payload.get("visibility", "private"),
                    pack_id=payload.get("id"),
                )
                self._send(201 if created else 200, {"created": created, "pack": manifest})
                return
            if parsed.path.startswith("/geopacks/") and parsed.path.endswith("/publish"):
                pack_id = parsed.path.split("/")[2]
                payload = self._read_json()
                relays = payload.get("relays") if payload else None
                self._send(200, self.store.publish_geopack(pack_id, relays=relays))
                return
            self._send_error(404, "not found")
        except ValueError as exc:
            self._send_error(400, str(exc))


def serve(host: str, port: int, store: VestaForgeStore, config: dict[str, Any]) -> None:
    handler = type(
        "ConfiguredVestaForgeHandler",
        (VestaForgeHandler,),
        {"store": store, "config": config},
    )
    httpd = ThreadingHTTPServer((host, port), handler)
    httpd.serve_forever()
