#!/usr/bin/env python3
"""Drive an MCP server over stdio, one awaited request at a time.

These calls depend on each other — `smix_find` has nothing to find until
`smix_use` has brought a runner up — and MCP requests are handled
concurrently. Written to the pipe all at once, `smix_release` answered
"nothing was bound" while `smix_use` was still inside xcodebuild, and
closing the pipe on that answer killed the call that mattered. A real
client waits for the reply it is about to act on, so this does too.

Writes every message it sees to <workdir>/out.jsonl, so a failure can be
read afterwards rather than re-run to be seen.

Usage: mcp-session-driver.py <mcp-binary> <udid> <port> <bundle-id> <workdir>
"""

import json
import os
import subprocess
import sys


def main() -> int:
    if len(sys.argv) != 6:
        print(__doc__, file=sys.stderr)
        return 2
    mcp, udid, port, bundle, work = sys.argv[1:6]

    err = open(os.path.join(work, "err.log"), "wb")
    proc = subprocess.Popen(
        [mcp],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=err,
        cwd=work,
        text=True,
        bufsize=1,
    )
    transcript = open(os.path.join(work, "out.jsonl"), "w")

    def send(msg: dict) -> None:
        assert proc.stdin is not None
        proc.stdin.write(json.dumps(msg) + "\n")
        proc.stdin.flush()

    def read_until(msg_id: int, what: str) -> dict:
        assert proc.stdout is not None
        while True:
            line = proc.stdout.readline()
            if not line:
                raise SystemExit(f"the server closed the stream before answering {what}")
            transcript.write(line)
            transcript.flush()
            try:
                msg = json.loads(line)
            except json.JSONDecodeError:
                continue
            if msg.get("id") == msg_id:
                return msg

    def call(msg_id: int, name: str, args: dict) -> dict:
        send(
            {
                "jsonrpc": "2.0",
                "id": msg_id,
                "method": "tools/call",
                "params": {"name": name, "arguments": args},
            }
        )
        return read_until(msg_id, name)

    send(
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "c4", "version": "0"},
            },
        }
    )
    read_until(1, "initialize")
    send({"jsonrpc": "2.0", "method": "notifications/initialized"})

    send({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
    read_until(2, "tools/list")

    # Unbound: this must be refused by naming the tool that binds one.
    call(3, "smix_tree", {})
    call(4, "smix_devices", {})
    # The long one: boots the device if needed, then xcodebuild until
    # /health answers.
    call(5, "smix_use", {"udid": udid, "port": int(port), "bundleId": bundle})
    call(6, "smix_find", {"id": "fixture-submit"})
    call(7, "smix_release", {})

    transcript.close()
    assert proc.stdin is not None
    proc.stdin.close()
    try:
        proc.wait(timeout=30)
    except subprocess.TimeoutExpired:
        proc.kill()
    err.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
