from __future__ import annotations

import argparse
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any


class MatrixStubHandler(BaseHTTPRequestHandler):
    provider: str
    domain: str

    def log_message(self, fmt: str, *args: Any) -> None:
        return

    def do_GET(self) -> None:  # noqa: N802 - stdlib handler naming.
        if self.path not in ("/", "/health", "/_matrix/client/versions"):
            self.send_response(404)
            self.end_headers()
            return
        payload = {
            "ok": True,
            "service": "matrix",
            "provider": self.provider,
            "domain": self.domain,
            "versions": ["r0.6.1", "v1.11"],
        }
        body = json.dumps(payload, sort_keys=True).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Mish Matrix provider health stub")
    parser.add_argument("--provider", required=True, choices=["tuwunel", "conduit"])
    parser.add_argument("--domain", required=True)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=6167)
    args = parser.parse_args(argv)

    handler = type(
        "ConfiguredMatrixStubHandler",
        (MatrixStubHandler,),
        {"provider": args.provider, "domain": args.domain},
    )
    ThreadingHTTPServer((args.host, args.port), handler).serve_forever()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
