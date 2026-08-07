#!/usr/bin/env bash
# v2.13-C4 MCP session e2e: a client that connects knowing nothing can
# list devices, pick one, bring its runner up, drive it, and let go.
#
# Deliberately runs with no SMIX_UDID. That variable used to be the only
# way the server learned which device to drive — set in the client's
# config file, before any conversation started, unchangeable without a
# restart. If it is set here, the test proves nothing about the case that
# matters: someone who just installed this and has not configured it.
#
# The transport is the real one: newline-delimited JSON-RPC over the
# server's stdin and stdout. Nothing here calls into the crate directly.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
MCP="${SMIX_MCP_BIN:-$ROOT/target/release/smix-mcp}"
SMIX="${SMIX_BIN:-$ROOT/target/release/smix}"
BUNDLE="jp.golia.smix.fixture"
APP="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
PORT=22091

log()  { printf '[c4-mcp] %s\n' "$*"; }
step() { printf '[c4-mcp] --- %s\n' "$*"; }
fail() { printf '[c4-mcp] FAIL: %s\n' "$*" >&2; exit 1; }

# A precondition this script detects and cannot satisfy is a SKIP with
# what to do about it — not a FAIL. Yielding to somebody else's batch, or
# an unset target, says nothing about whether smix works, and FAIL says it
# does not to whoever reads the suite next.
skip() { printf '[c4-mcp] %s\n' "$*" >&2; printf '%s\n' "C4-MCP-SESSION-SKIP"; exit 0; }


[ -x "$MCP" ] || fail "smix-mcp missing: $MCP (cargo build -p smix-mcp --release)"
[ -x "$SMIX" ] || fail "smix missing: $SMIX"

log "guard: no batch owner on this machine (yield, never seize)"
pgrep -f 'runner.ts|smix run|supervise' >/dev/null && skip "batch owner active — yielding"

UDID="${SMIX_C4_SIM:-}"
if [ -z "$UDID" ]; then
  UDID="$(bash "$ROOT/scripts/dev/pick-dev-sim.sh")" || skip "set SMIX_C4_SIM to a UDID"
fi
log "device: $UDID"

WORK="$(mktemp -d)"
cleanup() {
  step "teardown"
  # Keep the transcript: judging happens on it, and a failure that takes
  # the evidence with it costs a whole re-run to look at.
  cp "$WORK/out.jsonl" /tmp/c4-out.jsonl 2>/dev/null || true
  cp "$WORK/err.log" /tmp/c4-err.log 2>/dev/null || true
  ( cd "$WORK" && SMIX_RUNNER_PORT="$PORT" "$SMIX" runner down >/dev/null 2>&1 ) || true
  rm -rf "$WORK"
}
trap cleanup EXIT

step "build the fixture app and install it"
bash "$ROOT/scripts/dev/build-fixture-app.sh" >"$WORK/fixture.log" 2>&1 \
  || { tail -5 "$WORK/fixture.log" >&2; fail "fixture build failed"; }
xcrun simctl boot "$UDID" >/dev/null 2>&1 || true
xcrun simctl install "$UDID" "$APP" >"$WORK/install.log" 2>&1 \
  || { cat "$WORK/install.log" >&2; fail "install failed"; }

# --- drive the server over stdio ----------------------------------------

# The driver sends one request at a time and waits for its reply; see its
# docstring for why that is not incidental.
step "one MCP session, no SMIX_UDID in the environment"
env -u SMIX_UDID python3 "$ROOT/scripts/dev/mcp-session-driver.py" \
  "$MCP" "$UDID" "$PORT" "$BUNDLE" "$WORK" \
  || { tail -10 "$WORK/err.log" >&2; fail "the MCP session did not complete"; }

# --- judge ---------------------------------------------------------------

judge() {
  python3 - "$WORK/out.jsonl" "$1" <<'PY'
import json, sys
want = int(sys.argv[2])
for line in open(sys.argv[1]):
    line = line.strip()
    if not line:
        continue
    try:
        msg = json.loads(line)
    except json.JSONDecodeError:
        continue
    if msg.get("id") == want:
        print(json.dumps(msg))
        break
PY
}

step "judge the replies"

[ -n "$(judge 1)" ] || fail "no reply to initialize"
log "initialize answered"

TOOLS="$(judge 2)"
for t in smix_devices smix_use smix_release; do
  case "$TOOLS" in
    *"$t"*) ;;
    *) fail "tools/list does not offer $t" ;;
  esac
done
log "lifecycle tools offered"

UNBOUND="$(judge 3)"
case "$UNBOUND" in
  *smix_use*) log "unbound call refused by naming smix_use" ;;
  *) fail "an unbound smix_tree should name smix_use; got: $UNBOUND" ;;
esac

DEVICES="$(judge 4)"
case "$DEVICES" in
  *"$UDID"*) log "smix_devices lists the device" ;;
  *) fail "smix_devices did not list $UDID" ;;
esac

USED="$(judge 5)"
case "$USED" in
  *"driving $UDID"*) log "smix_use bound and brought the runner up" ;;
  *) fail "smix_use did not bind; got: $USED" ;;
esac

FOUND="$(judge 6)"
case "$FOUND" in
  *true*) log "drove the app through the session binding" ;;
  *) fail "smix_find did not see the fixture button; got: $FOUND" ;;
esac

RELEASED="$(judge 7)"
case "$RELEASED" in
  *"released $UDID"*) log "smix_release let go" ;;
  *) fail "smix_release did not release; got: $RELEASED" ;;
esac

if pgrep -f "xcodebuild.*SmixRunner" >/dev/null; then
  fail "a runner survived smix_release"
fi

log "C4-MCP-SESSION-PASS"
