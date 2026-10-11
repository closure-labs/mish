from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

from .server import serve
from .storage import VestaForgeStore, load_config, status_payload
from .validation import validate_catalog, validate_geopack_manifest, validate_waypoint


def print_json(value: Any) -> None:
    print(json.dumps(value, indent=2, sort_keys=True))


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="vfctl", description="VestaForge alpha control CLI")
    parser.add_argument("--config", help="Path to VestaForge config JSON")
    parser.add_argument("--state-dir", help="Override VestaForge state directory")
    sub = parser.add_subparsers(dest="command", required=True)

    commission = sub.add_parser("commission", help="Create first local operator; print token once")
    commission.add_argument("--name", required=True)
    enroll = sub.add_parser("enroll", help="Enroll staff using privileged local appliance access")
    enroll.add_argument("--name", required=True)
    enroll.add_argument("--role", choices=["staff", "operator"], default="staff")
    revoke = sub.add_parser("revoke", help="Revoke an enrolled token using local owner access")
    revoke.add_argument("--id", required=True)
    message = sub.add_parser("message", help="Write a message locally")
    message.add_argument("--text", required=True)
    message.add_argument("--author", required=True)
    alert = sub.add_parser("alert", help="Create a local operational alert")
    alert.add_argument("--text", required=True)
    alert.add_argument("--author", required=True)
    alert.add_argument("--responder", required=True)
    alert.add_argument("--expires", type=float, required=True)
    alert.add_argument("--urgent", action="store_true")
    ack = sub.add_parser("ack", help="Record an acknowledgement using enrolled bearer token")
    ack.add_argument("--id", required=True)
    ack.add_argument("--token", required=True)
    publish = sub.add_parser("announce", help="Explicitly publish official announcement locally")
    publish.add_argument("--text", required=True)
    backup = sub.add_parser("backup", help="Create consistent SQLite backup including credentials")
    backup.add_argument("--output", required=True)
    sub.add_parser("messages", help="List private local journal")
    sub.add_parser("prune", help="Apply configured event retention")
    wipe = sub.add_parser("wipe", help="Explicit end-of-event deletion; stop service first")
    wipe.add_argument("--confirm", choices=["WIPE-EVENT"], required=True)

    sub.add_parser("status", help="Show profile, provider, and service health")

    serve_parser = sub.add_parser("serve", help="Run the local VestaForge HTTP API")
    serve_parser.add_argument("--host", default=None)
    serve_parser.add_argument("--port", type=int, default=None)

    waypoint = sub.add_parser("waypoint", help="Manage waypoints")
    waypoint_sub = waypoint.add_subparsers(dest="waypoint_command", required=True)
    add_sim = waypoint_sub.add_parser("add-simulated", help="Add a simulated Meshtastic-like waypoint")
    add_sim.add_argument("--id", dest="waypoint_id")
    add_sim.add_argument("--lat", type=float, required=True)
    add_sim.add_argument("--lon", type=float, required=True)
    add_sim.add_argument("--label")
    add_sim.add_argument("--body")
    add_sim.add_argument(
        "--visibility",
        choices=["public", "friends", "private"],
        default="private",
    )
    waypoint_sub.add_parser("list", help="List local waypoints")

    geopack = sub.add_parser("geopack", help="Manage GeoPacks")
    geopack_sub = geopack.add_subparsers(dest="geopack_command", required=True)
    gp_create = geopack_sub.add_parser("create", help="Create a signed GeoPack manifest")
    gp_create.add_argument("--id", dest="pack_id")
    gp_create.add_argument("--name", required=True)
    gp_create.add_argument("--waypoint", action="append", required=True)
    gp_create.add_argument(
        "--visibility",
        choices=["public", "friends", "private"],
        default="private",
    )
    gp_publish = geopack_sub.add_parser("publish", help="Publish a public GeoPack manifest")
    gp_publish.add_argument("--pack", required=True)
    gp_publish.add_argument("--relay", action="append")
    geopack_sub.add_parser("list", help="List local GeoPacks")

    identity = sub.add_parser("identity", help="Manage appliance identities")
    identity_sub = identity.add_subparsers(dest="identity_command", required=True)
    identity_sub.add_parser("bootstrap-operator", help="Create operator enrollment bootstrap material")

    check = sub.add_parser("internal-check-schemas", help=argparse.SUPPRESS)
    check.add_argument("--schemas", required=True)
    check.add_argument("--examples", required=True)
    check.add_argument("--catalog", required=True)
    return parser


def load_store(args: argparse.Namespace) -> tuple[dict[str, Any], VestaForgeStore]:
    config = load_config(args.config, args.state_dir)
    return config, VestaForgeStore(config["stateDir"], config)


def command_status(args: argparse.Namespace) -> int:
    config, _store = load_store(args)
    print_json(status_payload(config, include_probes=True))
    return 0


def command_serve(args: argparse.Namespace) -> int:
    config, store = load_store(args)
    host = args.host or config.get("api", {}).get("bindHost", "127.0.0.1")
    port = args.port or int(config.get("api", {}).get("port", 8787))
    serve(host, port, store, config)
    return 0


def command_waypoint(args: argparse.Namespace) -> int:
    _config, store = load_store(args)
    if args.waypoint_command == "add-simulated":
        waypoint, created = store.add_simulated_waypoint(
            lat=args.lat,
            lon=args.lon,
            label=args.label,
            body=args.body,
            visibility=args.visibility,
            waypoint_id=args.waypoint_id,
            source_payload={"cli": "vfctl waypoint add-simulated"},
        )
        print_json({"created": created, "waypoint": waypoint})
        return 0
    if args.waypoint_command == "list":
        print_json({"waypoints": store.list_waypoints()})
        return 0
    raise AssertionError(args.waypoint_command)


def command_geopack(args: argparse.Namespace) -> int:
    _config, store = load_store(args)
    if args.geopack_command == "create":
        pack, created = store.create_geopack(
            name=args.name,
            waypoint_ids=args.waypoint,
            visibility=args.visibility,
            pack_id=args.pack_id,
        )
        print_json({"created": created, "pack": pack})
        return 0
    if args.geopack_command == "publish":
        print_json(store.publish_geopack(args.pack, relays=args.relay))
        return 0
    if args.geopack_command == "list":
        print_json({"geopacks": store.list_geopacks()})
        return 0
    raise AssertionError(args.geopack_command)


def command_identity(args: argparse.Namespace) -> int:
    _config, store = load_store(args)
    if args.identity_command == "bootstrap-operator":
        enrollment, created = store.bootstrap_operator()
        print_json({"created": created, "operatorEnrollment": enrollment})
        return 0
    raise AssertionError(args.identity_command)


def _load_json(path: Path) -> Any:
    with path.open("r", encoding="utf-8") as fh:
        return json.load(fh)


def command_internal_check_schemas(args: argparse.Namespace) -> int:
    schema_dir = Path(args.schemas)
    examples_dir = Path(args.examples)
    for schema in ("waypoint.schema.json", "geopack-manifest.schema.json", "catalog.schema.json"):
        _load_json(schema_dir / schema)

    for path in (examples_dir / "waypoints").glob("*.json"):
        validate_waypoint(_load_json(path))
    for path in (examples_dir / "geopacks").glob("*.json"):
        validate_geopack_manifest(_load_json(path))
    validate_catalog(_load_json(Path(args.catalog)))
    print_json({"ok": True, "schemas": str(schema_dir), "examples": str(examples_dir)})
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        if args.command in {"commission", "enroll", "revoke", "message", "alert", "ack", "announce", "backup", "messages", "prune", "wipe"}:
            from .appliance import EventStore
            config = load_config(args.config, args.state_dir)
            if args.command == "wipe" and not (Path(config["stateDir"]) / "events.sqlite").is_file():
                raise ValueError("refusing to wipe an uncommissioned state directory")
            events = EventStore(config["stateDir"], config.get("queueLimit", 1000), recover=False)
            if args.command == "commission":
                print_json(events.enroll(args.name, "operator", first=True))
            elif args.command == "enroll":
                print_json(events.enroll(args.name, args.role))
            elif args.command == "revoke":
                events.revoke(args.id)
                print_json({"revoked": args.id})
            elif args.command == "message":
                print_json(events.create(args.text, args.author))
            elif args.command == "alert":
                print_json(events.create(args.text, args.author, "urgent" if args.urgent else "alert", args.responder, args.expires))
            elif args.command == "ack":
                actor = events.authenticate(args.token)
                if not actor:
                    raise ValueError("invalid staff token")
                print_json(events.ack(args.id, actor["id"]))
            elif args.command == "announce":
                print_json(events.publish(args.text))
            elif args.command == "backup":
                events.backup(args.output)
                print_json({"backup": args.output})
            elif args.command == "messages":
                print_json({"messages": events.records()})
            elif args.command == "prune":
                events.prune(config.get("retentionDays", 7))
            elif args.command == "wipe":
                import shutil
                # State path comes from privileged CLI config, never a network request.
                state = Path(config["stateDir"]).resolve()
                if str(state) in ("/", "/var", "/var/lib", "/home", "/tmp"):
                    raise ValueError("refusing unsafe state directory")
                for child in state.iterdir():
                    if child.is_dir() and not child.is_symlink():
                        shutil.rmtree(child)
                    else:
                        child.unlink()
                print_json({"wiped": str(state)})
            return 0
        if args.command == "status":
            return command_status(args)
        if args.command == "serve":
            return command_serve(args)
        if args.command == "waypoint":
            return command_waypoint(args)
        if args.command == "geopack":
            return command_geopack(args)
        if args.command == "identity":
            return command_identity(args)
        if args.command == "internal-check-schemas":
            return command_internal_check_schemas(args)
    except ValueError as exc:
        print(f"vfctl: {exc}", file=sys.stderr)
        return 2
    parser.error("unknown command")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
