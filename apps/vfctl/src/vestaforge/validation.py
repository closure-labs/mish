from __future__ import annotations

from typing import Any

VALID_VISIBILITY = {"public", "friends", "private"}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def validate_waypoint(value: dict[str, Any]) -> None:
    for field in ("id", "createdAt", "source", "visibility", "position", "provenance"):
        require(field in value, f"waypoint missing {field}")
    require(value["visibility"] in VALID_VISIBILITY, "invalid waypoint visibility")
    position = value["position"]
    require(isinstance(position, dict), "waypoint position must be an object")
    require(-90 <= float(position["lat"]) <= 90, "latitude out of range")
    require(-180 <= float(position["lon"]) <= 180, "longitude out of range")


def validate_geopack_manifest(value: dict[str, Any]) -> None:
    required = (
        "version",
        "id",
        "name",
        "createdAt",
        "waypointIds",
        "contentHash",
        "signer",
        "publication",
        "signature",
    )
    for field in required:
        require(field in value, f"GeoPack manifest missing {field}")
    require(value["version"] == "0.1", "GeoPack version must be 0.1")
    require(str(value["contentHash"]).startswith("sha256:"), "contentHash must be sha256-prefixed")
    require(str(value["signature"]).startswith("hmac-sha256:"), "signature must be hmac-sha256-prefixed")
    require(isinstance(value["waypointIds"], list) and value["waypointIds"], "waypointIds must be a non-empty list")
    publication = value["publication"]
    require(publication["visibility"] in VALID_VISIBILITY, "invalid GeoPack publication visibility")


def validate_catalog(value: dict[str, Any]) -> None:
    for field in ("version", "signedAt", "signer", "profiles", "capabilities", "policies", "signature"):
        require(field in value, f"catalog missing {field}")
    profiles = value["profiles"]
    for profile in ("mini-pc", "sbc-rpi4", "home-server"):
        require(profile in profiles, f"catalog must include {profile} profile")
        require(isinstance(profiles[profile], dict), f"catalog {profile} profile must be an object")
    require(profiles["mini-pc"].get("defaultMatrixProvider") == "tuwunel", "mini-pc must default to Tuwunel")
    require(profiles["sbc-rpi4"].get("defaultMatrixProvider") == "conduit", "sbc-rpi4 must default to Conduit")
    require(profiles["mini-pc"].get("releaseGated") is True, "mini-pc must be release-gated")
    require(profiles["sbc-rpi4"].get("releaseGated") is True, "sbc-rpi4 must be release-gated")
    require(profiles["home-server"].get("releaseGated") is False, "home-server must not be alpha release-gated")
    matrix_policy = value["policies"].get("matrix")
    require(isinstance(matrix_policy, dict), "catalog must include matrix policy")
    require(matrix_policy.get("denyDualBackend") is True, "catalog must deny Matrix dual backend")
    radio = value["capabilities"].get("radio")
    require(isinstance(radio, dict), "catalog must include radio capability")
    providers = radio.get("providers", [])
    require("meshtastic-simulated" in providers, "catalog radio must include meshtastic-simulated")
    require("reticulum-rnode" in providers, "catalog radio must include reticulum-rnode")
    require(
        "meshtastic-simulated" in radio.get("releaseGatedProviders", []),
        "catalog radio must release-gate meshtastic-simulated",
    )
    require(
        "reticulum-rnode" in radio.get("experimentalProviders", []),
        "catalog radio must mark reticulum-rnode experimental",
    )
