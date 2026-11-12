#!/usr/bin/env bash
# v26.0-C1: an MCP session that goes quiet gives its device back.
#
# A consumer's MCP server bound a simulator and then sat for fourteen
# hours with no command; their release round's runner met the lease on
# every one of 31 flows and ran none. On a real simulator, through the
# server's real stdio transport, with SMIX_MCP_IDLE_RELEASE=30s:
#   1. while the server holds the device, `smix run` is refused with exit
#      7, naming the server, how long since its ledger was written, and
#      how such a server lets go
#   2. after 30 s without a command the server gives the device back by
#      itself: its next tool call says so and names smix_use
#   3. a second session then brings a runner up and runs a flow on the
#      device, exit 0 — the server, still running, is not in its way
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port PORT
BUNDLE="jp.golia.smix.fixture"
APP="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
IDLE=30
WORK="$(mktemp -d)"

log()  { printf '[v26.0-c1-a] %s\n' "$*" >&2; }
fail() { printf '[v26.0-c1-a] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v26.0-c1-a] cannot judge: %s\n' "$*" >&2; exit 2; }

UDID="" WE_BOOTED=0
cleanup() {
  local said
  if ! said="$("$SMIX" runner down --runner-port "$PORT" 2>&1)"; then
    log "warning: the runner on $PORT was not stopped:"
    printf '%s\n' "$said" | tail -3 >&2
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

[ -x "$SMIX_MCP" ] || fail "no smix-mcp at $SMIX_MCP (cargo build -p smix-mcp)"
UDID="$("$SMIX" sim resolve "$E2E_IOS" 2>/dev/null | tr -d '[:space:]')" || true
[ -n "$UDID" ] || cannot_judge "no simulator registered as '$E2E_IOS'"
e2e_yield_if_held "$UDID"

bash "$ROOT/scripts/dev/build-fixture-app.sh" >"$WORK/fixture.log" 2>&1 \
  || fail "fixture build: $(tail -5 "$WORK/fixture.log")"
if [ "$(simulator_state "$UDID")" != Booted ]; then
  "$SMIX" sim boot "$UDID" >"$WORK/boot.log" 2>&1 || fail "sim boot: $(tail -3 "$WORK/boot.log")"
  WE_BOOTED=1
fi
xcrun simctl install "$UDID" "$APP" >"$WORK/install.log" 2>&1 || fail "install: $(cat "$WORK/install.log")"

log "one MCP session on $UDID, port $PORT, idle limit ${IDLE}s"
python3 "$ROOT/scripts/dev/mcp-idle-driver.py" "$SMIX_MCP" "$SMIX" "$UDID" "$PORT" "$BUNDLE" "$WORK" "$IDLE" \
  || fail "the session did not complete: $(tail -10 "$WORK/err.log")"

python3 - "$WORK/seen.json" <<'PY' || fail "see above"
import json, sys
seen = json.load(open(sys.argv[1]))
bad = []
def check(ok, what):
    print(("  ok    " if ok else "  WRONG ") + what, file=sys.stderr)
    if not ok:
        bad.append(what)

check('"isError": false' in seen["use"], "smix_use bound the device: " + seen["use"][-160:])
held = seen["runWhileBound"]
check(held["exit"] == 7, f"while bound, smix run exits 7 (got {held['exit']})")
for words in ("in use by pid", "smix-mcp", "ledger last written", "SMIX_MCP_IDLE_RELEASE"):
    check(words in held["stderr"], f"the refusal says {words!r}")
after = seen["findAfterIdle"]
check("given back after" in after and "smix_use" in after, "the next tool call says the device was given back")
check(seen["mcpAliveAfterIdle"], "the server is still running")
up, run = seen["upAfterIdle"], seen["runAfterIdle"]
check(up["exit"] == 0, f"a second session brings a runner up (exit {up['exit']}): {up['stderr'][-300:]}")
check(run["exit"] == 0, f"and runs a flow on the device (exit {run['exit']}): {run['stderr'][-300:]}")
sys.exit(1 if bad else 0)
PY
log "PASS"
