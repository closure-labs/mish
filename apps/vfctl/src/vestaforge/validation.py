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
    require("mini-pc" in value["profiles"], "catalog must include mini-pc profile")
    matrix_policy = value["policies"].get("matrix")
    require(isinstance(matrix_policy, dict), "catalog must include matrix policy")
    require(matrix_policy.get("denyDualBackend") is True, "catalog must deny Matrix dual backend")
