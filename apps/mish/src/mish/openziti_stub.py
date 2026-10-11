from __future__ import annotations

import argparse
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any


class OpenZitiStubHandler(BaseHTTPRequestHandler):
    role: str
    privileged_mfa_required: bool

    def log_message(self, fmt: str, *args: Any) -> None:
        return

    def do_GET(self) -> None:  # noqa: N802 - stdlib handler naming.
        if self.path not in ("/", "/health"):
            self.send_response(404)
            self.end_headers()
            return
        payload = {
            "ok": True,
            "service": "openziti",
            "role": self.role,
            "privilegedMfaRequired": self.privileged_mfa_required,
        }
        body = json.dumps(payload, sort_keys=True).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Mish OpenZiti baseline health stub")
    parser.add_argument("--role", required=True, choices=["controller", "edge-router"])
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--require-mfa", action="store_true")
    args = parser.parse_args(argv)

    handler = type(
        "ConfiguredOpenZitiStubHandler",
        (OpenZitiStubHandler,),
        {"role": args.role, "privileged_mfa_required": args.require_mfa},
    )
    ThreadingHTTPServer((args.host, args.port), handler).serve_forever()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
