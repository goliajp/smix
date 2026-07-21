#!/usr/bin/env python3
"""Forward to the runner and record what the driver actually sent.

`smix run` does not show you the wire, so an assertion about a header
the driver is supposed to attach has nothing to read. This sits between
them: the flow talks to this port, this talks to the runner, and every
exchange lands in a JSONL file.

Instrument, not product — it lives in scripts/dev/ and nothing in
crates/ knows it exists.

Usage:
  android-wire-record.py --listen 28090 --forward 28080 --out /tmp/wire.jsonl
"""

import argparse
import http.server
import json
import sys
import threading
import urllib.error
import urllib.request

RECORDED_REQUEST_HEADERS = ("App-Bundle-Id", "Input-Dispatch-Mode", "Session-Id")
RECORDED_RESPONSE_HEADERS = ("X-View-Id-Match",)


class Recorder(http.server.BaseHTTPRequestHandler):
    forward_port = 28080
    out_path = "/tmp/smix-wire.jsonl"
    lock = threading.Lock()

    def log_message(self, *_args):
        pass  # the JSONL is the log

    def _relay(self, method):
        length = int(self.headers.get("Content-Length") or 0)
        payload = self.rfile.read(length) if length else None

        upstream = urllib.request.Request(
            f"http://localhost:{self.forward_port}{self.path}",
            data=payload,
            method=method,
        )
        for name, value in self.headers.items():
            if name.lower() in ("host", "content-length"):
                continue
            upstream.add_header(name, value)

        record = {
            "method": method,
            "path": self.path,
            "request_headers": {
                h: self.headers.get(h) for h in RECORDED_REQUEST_HEADERS
            },
        }

        try:
            with urllib.request.urlopen(upstream, timeout=120) as response:
                body = response.read()
                status = response.status
                headers = dict(response.headers)
        except urllib.error.HTTPError as e:
            body = e.read()
            status = e.code
            headers = dict(e.headers)
        except Exception as e:  # upstream gone: record it, do not vanish
            record["error"] = str(e)
            self._write(record)
            self.send_error(502, str(e))
            return

        record["status"] = status
        record["response_headers"] = {
            h: headers.get(h) for h in RECORDED_RESPONSE_HEADERS
        }
        self._write(record)

        self.send_response(status)
        for name, value in headers.items():
            if name.lower() in ("transfer-encoding", "content-length", "connection"):
                continue
            self.send_header(name, value)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _write(self, record):
        with self.lock:
            with open(self.out_path, "a", encoding="utf-8") as f:
                f.write(json.dumps(record) + "\n")

    def do_GET(self):
        self._relay("GET")

    def do_POST(self):
        self._relay("POST")

    def do_DELETE(self):
        self._relay("DELETE")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--listen", type=int, default=28090)
    ap.add_argument("--forward", type=int, default=28080)
    ap.add_argument("--out", default="/tmp/smix-wire.jsonl")
    args = ap.parse_args()

    Recorder.forward_port = args.forward
    Recorder.out_path = args.out
    open(args.out, "w").close()

    server = http.server.ThreadingHTTPServer(("127.0.0.1", args.listen), Recorder)
    print(f"android-wire-record: {args.listen} -> {args.forward}, recording to {args.out}")
    sys.stdout.flush()
    server.serve_forever()


if __name__ == "__main__":
    main()
