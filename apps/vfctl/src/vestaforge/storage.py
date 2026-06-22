from __future__ import annotations

import base64
import datetime as _dt
import hashlib
import hmac
import json
import os
import secrets
import tempfile
import time
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any

VERSION = "0.1.0-alpha"
FORMAT_VERSION = "0.1"
VALID_VISIBILITY = {"public", "friends", "private"}


def utc_now() -> str:
    fixed = os.environ.get("VF_FIXED_TIME")
    if fixed:
        return fixed
    return (
        _dt.datetime.now(_dt.timezone.utc)
        .replace(microsecond=0)
        .isoformat()
        .replace("+00:00", "Z")
    )


def unix_now() -> int:
    fixed = os.environ.get("VF_FIXED_UNIX_TIME")
    if fixed:
        return int(fixed)
    return int(time.time())


def canonical_json(value: Any) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def sha256_hex(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_json(value: Any) -> str:
    return sha256_hex(canonical_json(value).encode("utf-8"))


def deep_merge(base: dict[str, Any], overlay: dict[str, Any]) -> dict[str, Any]:
    result = dict(base)
    for key, value in overlay.items():
        if isinstance(value, dict) and isinstance(result.get(key), dict):
            result[key] = deep_merge(result[key], value)
        else:
            result[key] = value
    return result


def default_config(state_dir: str | None = None) -> dict[str, Any]:
    state = state_dir or os.environ.get("VESTAFORGE_STATE_DIR", "/var/lib/vestaforge")
    return {
        "version": VERSION,
        "profile": "mini-pc",
        "domain": "vestaforge.local",
        "stateDir": state,
        "api": {"bindHost": "127.0.0.1", "port": 8787},
        "matrix": {
            "provider": "tuwunel",
            "domain": "vestaforge.local",
            "port": 6167,
            "homeserverUrl": "http://127.0.0.1:6167",
            "releaseGated": True,
        },
        "openziti": {
            "enable": True,
            "controllerUrl": "http://127.0.0.1:1280",
            "edgeRouterUrl": "http://127.0.0.1:3022",
            "privilegedMfaRequired": True,
            "serviceIdentities": ["matrix", "geopack", "admin-api"],
        },
        "geopack": {"enable": True},
        "nostr": {"relays": []},
        "radio": {"simulator": {"enable": True}},
        "catalog": {"path": ""},
    }


def load_config(config_path: str | None = None, state_dir: str | None = None) -> dict[str, Any]:
    path = config_path or os.environ.get("VESTAFORGE_CONFIG", "/etc/vestaforge/config.json")
    base = default_config(state_dir)
    if path and Path(path).exists():
        with Path(path).open("r", encoding="utf-8") as fh:
            loaded = json.load(fh)
        base = deep_merge(base, loaded)
    if state_dir:
        base["stateDir"] = state_dir
    return base


def ensure_visibility(value: str) -> str:
    if value not in VALID_VISIBILITY:
        raise ValueError(f"visibility must be one of: {', '.join(sorted(VALID_VISIBILITY))}")
    return value


def validate_position(lat: float, lon: float) -> tuple[float, float]:
    lat_f = float(lat)
    lon_f = float(lon)
    if not -90.0 <= lat_f <= 90.0:
        raise ValueError("latitude must be between -90 and 90")
    if not -180.0 <= lon_f <= 180.0:
        raise ValueError("longitude must be between -180 and 180")
    return lat_f, lon_f


def _atomic_write_json(path: Path, value: Any, mode: int = 0o600) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp_name = tempfile.mkstemp(prefix=f".{path.name}.", dir=str(path.parent))
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as fh:
            json.dump(value, fh, indent=2, sort_keys=True)
            fh.write("\n")
        os.chmod(tmp_name, mode)
        os.replace(tmp_name, path)
    finally:
        if os.path.exists(tmp_name):
            os.unlink(tmp_name)


def _read_json(path: Path) -> Any:
    with path.open("r", encoding="utf-8") as fh:
        return json.load(fh)


def _slug(value: str) -> str:
    cleaned = "".join(ch.lower() if ch.isalnum() else "-" for ch in value)
    cleaned = "-".join(part for part in cleaned.split("-") if part)
    return cleaned or "geopack"


class VestaForgeStore:
    def __init__(self, state_dir: str | Path, config: dict[str, Any] | None = None) -> None:
        self.state_dir = Path(state_dir)
        self.config = config or default_config(str(self.state_dir))
        self.waypoints_dir = self.state_dir / "waypoints"
        self.geopacks_dir = self.state_dir / "geopacks"
        self.publications_dir = self.state_dir / "nostr" / "publications"
        self.keys_dir = self.state_dir / "keys"
        self.openziti_dir = self.state_dir / "openziti"
        for directory in (
            self.waypoints_dir,
            self.geopacks_dir,
            self.publications_dir,
            self.keys_dir,
            self.openziti_dir,
        ):
            directory.mkdir(parents=True, exist_ok=True)

    def waypoint_path(self, waypoint_id: str) -> Path:
        return self.waypoints_dir / f"{waypoint_id}.json"

    def geopack_path(self, pack_id: str) -> Path:
        return self.geopacks_dir / f"{pack_id}.json"

    def signing_key_path(self) -> Path:
        return self.keys_dir / "geopack-signing.key"

    def _read_signing_key(self) -> bytes:
        path = self.signing_key_path()
        if path.exists():
            raw = path.read_text(encoding="utf-8").strip()
            return base64.b64decode(raw.encode("ascii"))
        key = secrets.token_bytes(32)
        fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(fd, "w", encoding="utf-8") as fh:
            fh.write(base64.b64encode(key).decode("ascii"))
            fh.write("\n")
        return key

    def signer(self) -> str:
        key = self._read_signing_key()
        return "vf:" + hashlib.sha256(key).hexdigest()[:32]

    def _signature(self, value: Any) -> str:
        key = self._read_signing_key()
        digest = hmac.new(key, canonical_json(value).encode("utf-8"), hashlib.sha256).hexdigest()
        return "hmac-sha256:" + digest

    def simulated_waypoint_id(
        self,
        lat: float,
        lon: float,
        label: str | None,
        body: str | None,
        source_payload: dict[str, Any] | None = None,
    ) -> str:
        material = {
            "body": body,
            "label": label,
            "lat": round(float(lat), 7),
            "lon": round(float(lon), 7),
            "payload": source_payload or {},
            "source": "meshtastic-simulated",
        }
        return "wp-" + sha256_json(material)[:24]

    def add_simulated_waypoint(
        self,
        lat: float,
        lon: float,
        label: str | None = None,
        body: str | None = None,
        visibility: str = "private",
        waypoint_id: str | None = None,
        source_payload: dict[str, Any] | None = None,
    ) -> tuple[dict[str, Any], bool]:
        visibility = ensure_visibility(visibility)
        lat_f, lon_f = validate_position(lat, lon)
        payload = source_payload or {}
        waypoint_id = waypoint_id or self.simulated_waypoint_id(lat_f, lon_f, label, body, payload)
        path = self.waypoint_path(waypoint_id)
        if path.exists():
            return _read_json(path), False

        created_at = utc_now()
        waypoint: dict[str, Any] = {
            "id": waypoint_id,
            "createdAt": created_at,
            "source": "meshtastic-simulated",
            "visibility": visibility,
            "position": {"lat": lat_f, "lon": lon_f},
            "provenance": {
                "adapter": "radio.simulator",
                "format": "meshtastic-position",
                "receivedAt": created_at,
                "payload": payload,
            },
        }
        if label:
            waypoint["label"] = label
        if body:
            waypoint["body"] = body
        _atomic_write_json(path, waypoint)
        return waypoint, True

    def list_waypoints(self) -> list[dict[str, Any]]:
        return [_read_json(path) for path in sorted(self.waypoints_dir.glob("*.json"))]

    def list_geopacks(self) -> list[dict[str, Any]]:
        return [_read_json(path) for path in sorted(self.geopacks_dir.glob("*.json"))]

    def load_waypoint(self, waypoint_id: str) -> dict[str, Any]:
        path = self.waypoint_path(waypoint_id)
        if not path.exists():
            raise ValueError(f"waypoint not found: {waypoint_id}")
        return _read_json(path)

    def load_geopack(self, pack_id: str) -> dict[str, Any]:
        path = self.geopack_path(pack_id)
        if not path.exists():
            raise ValueError(f"GeoPack not found: {pack_id}")
        return _read_json(path)

    def _content_projection(self, waypoint: dict[str, Any]) -> dict[str, Any]:
        projected: dict[str, Any] = {
            "id": waypoint["id"],
            "source": waypoint["source"],
            "visibility": waypoint["visibility"],
            "position": waypoint["position"],
        }
        for optional in ("label", "body"):
            if optional in waypoint:
                projected[optional] = waypoint[optional]
        provenance = waypoint.get("provenance", {})
        projected["provenance"] = {
            "adapter": provenance.get("adapter"),
            "format": provenance.get("format"),
            "payload": provenance.get("payload", {}),
        }
        return projected

    def _geopack_hash(self, waypoints: list[dict[str, Any]]) -> str:
        content = {
            "format": "vestaforge-geopack-content",
            "waypoints": [self._content_projection(wp) for wp in sorted(waypoints, key=lambda wp: wp["id"])],
        }
        return "sha256:" + sha256_json(content)

    def _sign_manifest(self, manifest: dict[str, Any]) -> dict[str, Any]:
        unsigned = {key: value for key, value in manifest.items() if key != "signature"}
        signed = dict(unsigned)
        signed["signature"] = self._signature(unsigned)
        return signed

    def create_geopack(
        self,
        name: str,
        waypoint_ids: list[str],
        visibility: str = "private",
        pack_id: str | None = None,
    ) -> tuple[dict[str, Any], bool]:
        if not name.strip():
            raise ValueError("GeoPack name is required")
        visibility = ensure_visibility(visibility)
        ids = sorted(dict.fromkeys(waypoint_ids))
        if not ids:
            raise ValueError("at least one waypoint is required")
        waypoints = [self.load_waypoint(waypoint_id) for waypoint_id in ids]
        if visibility == "public":
            non_public = [wp["id"] for wp in waypoints if wp.get("visibility") != "public"]
            if non_public:
                raise ValueError(
                    "public GeoPacks may only include public waypoints: " + ", ".join(non_public)
                )
        content_hash = self._geopack_hash(waypoints)
        pack_id = pack_id or f"{_slug(name)}-{sha256_json({'name': name, 'waypoints': ids, 'hash': content_hash})[:12]}"
        path = self.geopack_path(pack_id)
        if path.exists():
            return _read_json(path), False
        manifest = {
            "version": FORMAT_VERSION,
            "id": pack_id,
            "name": name,
            "createdAt": utc_now(),
            "waypointIds": ids,
            "contentHash": content_hash,
            "signer": self.signer(),
            "publication": {
                "visibility": visibility,
                "publishedAt": None,
                "relays": [],
                "eventId": None,
            },
        }
        signed = self._sign_manifest(manifest)
        _atomic_write_json(path, signed)
        return signed, True

    def publish_geopack(self, pack_id: str, relays: list[str] | None = None) -> dict[str, Any]:
        manifest = self.load_geopack(pack_id)
        publication = manifest.get("publication", {})
        if publication.get("visibility") != "public":
            raise ValueError("GeoPack publication.visibility must be public before publishing")
        waypoints = [self.load_waypoint(waypoint_id) for waypoint_id in manifest["waypointIds"]]
        non_public = [wp["id"] for wp in waypoints if wp.get("visibility") != "public"]
        if non_public:
            raise ValueError("private or friend-scoped waypoints cannot be published: " + ", ".join(non_public))

        configured_relays = relays if relays is not None else self.config.get("nostr", {}).get("relays", [])
        configured_relays = configured_relays or ["mock://local"]
        event = self._build_publication_event(manifest)
        statuses = [self._publish_to_relay(relay, event, pack_id) for relay in configured_relays]

        updated = dict(manifest)
        updated["publication"] = dict(publication)
        updated["publication"].update(
            {
                "publishedAt": utc_now(),
                "relays": configured_relays,
                "eventId": event["id"],
            }
        )
        updated = self._sign_manifest(updated)
        _atomic_write_json(self.geopack_path(pack_id), updated)
        return {"pack": updated, "event": event, "relays": statuses}

    def _build_publication_event(self, manifest: dict[str, Any]) -> dict[str, Any]:
        unsigned = {
            "pubkey": manifest["signer"],
            "created_at": unix_now(),
            "kind": 30078,
            "tags": [
                ["d", manifest["id"]],
                ["application", "vestaforge"],
                ["format", "geopack-manifest"],
                ["contentHash", manifest["contentHash"]],
            ],
            "content": canonical_json(manifest),
        }
        event_id = sha256_json(unsigned)
        event = dict(unsigned)
        event["id"] = event_id
        event["sig"] = self._signature(unsigned)
        return event

    def _publish_to_relay(self, relay: str, event: dict[str, Any], pack_id: str) -> dict[str, Any]:
        if relay.startswith("mock://"):
            target = self.publications_dir / f"{pack_id}.json"
            _atomic_write_json(target, {"relay": relay, "event": event})
            return {"relay": relay, "status": "stored", "path": str(target)}
        if relay.startswith("file://"):
            parsed = urllib.parse.urlparse(relay)
            target = Path(urllib.request.url2pathname(parsed.path))
            if target.suffix:
                target.parent.mkdir(parents=True, exist_ok=True)
                with target.open("a", encoding="utf-8") as fh:
                    fh.write(canonical_json(event))
                    fh.write("\n")
                return {"relay": relay, "status": "appended", "path": str(target)}
            target.mkdir(parents=True, exist_ok=True)
            file_target = target / f"{pack_id}.json"
            _atomic_write_json(file_target, {"relay": relay, "event": event})
            return {"relay": relay, "status": "stored", "path": str(file_target)}
        outbox = self.publications_dir / f"{pack_id}.outbox.json"
        _atomic_write_json(outbox, {"relay": relay, "event": event})
        return {"relay": relay, "status": "queued", "path": str(outbox)}

    def bootstrap_operator(self) -> tuple[dict[str, Any], bool]:
        path = self.openziti_dir / "operator-enrollment.json"
        if path.exists():
            return _read_json(path), False
        enrollment = {
            "version": FORMAT_VERSION,
            "identity": "operator",
            "createdAt": utc_now(),
            "enrollmentToken": secrets.token_urlsafe(32),
            "mfaRequired": bool(
                self.config.get("openziti", {}).get("privilegedMfaRequired", True)
            ),
            "policies": ["privileged-admin", "service-dial", "operator-bootstrap"],
        }
        _atomic_write_json(path, enrollment)
        return enrollment, True


def probe_http_json(url: str, timeout: float = 0.5) -> dict[str, Any]:
    try:
        with urllib.request.urlopen(url, timeout=timeout) as response:
            body = response.read().decode("utf-8")
        return {"ok": True, "status": "reachable", "response": json.loads(body)}
    except Exception as exc:  # noqa: BLE001 - health probes should report all failures.
        return {"ok": False, "status": "unreachable", "error": str(exc)}


def status_payload(config: dict[str, Any], include_probes: bool = True) -> dict[str, Any]:
    matrix = dict(config.get("matrix", {}))
    openziti = dict(config.get("openziti", {}))
    payload = {
        "ok": True,
        "version": config.get("version", VERSION),
        "profile": config.get("profile", "mini-pc"),
        "domain": config.get("domain", "vestaforge.local"),
        "stateDir": config.get("stateDir"),
        "matrix": {
            "provider": matrix.get("provider"),
            "domain": matrix.get("domain"),
            "releaseGated": bool(matrix.get("releaseGated")),
        },
        "openziti": {
            "enabled": bool(openziti.get("enable")),
            "privilegedMfaRequired": bool(openziti.get("privilegedMfaRequired", True)),
            "serviceIdentities": openziti.get("serviceIdentities", []),
        },
        "geopack": {"enabled": bool(config.get("geopack", {}).get("enable", True))},
        "nostr": {"relays": config.get("nostr", {}).get("relays", [])},
        "radio": {"simulator": config.get("radio", {}).get("simulator", {})},
    }
    if include_probes:
        matrix_url = matrix.get("homeserverUrl")
        if matrix_url:
            payload["matrix"]["health"] = probe_http_json(matrix_url.rstrip("/") + "/health")
        if openziti.get("enable"):
            payload["openziti"]["controllerHealth"] = probe_http_json(
                str(openziti.get("controllerUrl", "")).rstrip("/") + "/health"
            )
            payload["openziti"]["edgeRouterHealth"] = probe_http_json(
                str(openziti.get("edgeRouterUrl", "")).rstrip("/") + "/health"
            )
    return payload
