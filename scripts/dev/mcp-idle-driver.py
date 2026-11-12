#!/usr/bin/env python3
"""Hold a device through an MCP session, go quiet, and see who can use it.

One awaited request at a time, for the reason mcp-session-driver.py
gives. Between requests it runs the CLI against the same device, because
the claim being judged is about a second session: what `smix run` meets
while the MCP server holds the device, and after the server has been
quiet past its idle limit.

Writes every MCP message to <workdir>/out.jsonl and a summary of what it
saw to <workdir>/seen.json.

Usage: mcp-idle-driver.py <mcp> <smix> <udid> <port> <bundle-id> <workdir> <idle-secs>
"""

import json
import os
import subprocess
import sys
import time


def main() -> int:
    if len(sys.argv) != 8:
        print(__doc__, file=sys.stderr)
        return 2
    mcp, smix, udid, port, bundle, work, idle = sys.argv[1:8]
    idle = int(idle)
    env = dict(os.environ, SMIX_MCP_IDLE_RELEASE=f"{idle}s")
    env.pop("SMIX_UDID", None)

    err = open(os.path.join(work, "err.log"), "wb")
    proc = subprocess.Popen(
        [mcp], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=err,
        cwd=work, text=True, bufsize=1, env=env,
    )
    transcript = open(os.path.join(work, "out.jsonl"), "w")
    seen: dict = {"mcpPid": proc.pid}

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

    def call(msg_id: int, name: str, args: dict) -> str:
        send({"jsonrpc": "2.0", "id": msg_id, "method": "tools/call",
              "params": {"name": name, "arguments": args}})
        return json.dumps(read_until(msg_id, name))

    def lease() -> dict:
        out = subprocess.run([smix, "lease", "status", udid, "--json"],
                             capture_output=True, text=True, cwd=work)
        try:
            return json.loads(out.stdout)
        except json.JSONDecodeError:
            return {"unreadable": out.stdout + out.stderr}

    def run_flow() -> dict:
        flow = os.path.join(work, "open.yaml")
        with open(flow, "w") as f:
            f.write(f"appId: {bundle}\n---\n- launchApp\n")
        out = subprocess.run([smix, "run", "--device", udid, flow], capture_output=True,
                             text=True, cwd=work, env=dict(os.environ, SMIX_RUNNER_PORT=port))
        return {"exit": out.returncode, "stderr": out.stderr[-2000:]}

    send({"jsonrpc": "2.0", "id": 1, "method": "initialize",
          "params": {"protocolVersion": "2024-11-05", "capabilities": {},
                     "clientInfo": {"name": "idle", "version": "0"}}})
    read_until(1, "initialize")
    send({"jsonrpc": "2.0", "method": "notifications/initialized"})

    seen["use"] = call(2, "smix_use", {"udid": udid, "port": int(port), "bundleId": bundle})
    seen["findWhileBound"] = call(3, "smix_find", {"id": "fixture-submit"})
    seen["leaseWhileBound"] = lease()
    seen["runWhileBound"] = run_flow()

    # The watch looks every quarter of the limit; give it the limit and
    # two looks, then a little for the runner to come down.
    deadline = time.time() + idle + idle / 2 + 60
    while time.time() < deadline:
        time.sleep(3)
        if b"given back after" in open(os.path.join(work, "err.log"), "rb").read():
            break
    seen["leaseAfterIdle"] = lease()
    seen["findAfterIdle"] = call(4, "smix_find", {"id": "fixture-submit"})
    # The server took its runner down with the lease, so a second session
    # brings its own up before it drives.
    up = subprocess.run([smix, "runner", "up", "--device", udid, "--runner-port", port,
                         "--bundle", bundle], capture_output=True, text=True, cwd=work)
    seen["upAfterIdle"] = {"exit": up.returncode, "stderr": up.stderr[-1500:]}
    seen["runAfterIdle"] = run_flow()
    subprocess.run([smix, "runner", "down", "--runner-port", port], capture_output=True, cwd=work)
    seen["mcpAliveAfterIdle"] = proc.poll() is None

    with open(os.path.join(work, "seen.json"), "w") as f:
        json.dump(seen, f, indent=1)
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
