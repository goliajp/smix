#!/usr/bin/env bash
# v2.13-C8 collaborative loop e2e: someone developing an app, in their own
# repo, with this plugin installed and nothing else.
#
# The standalone loop (C3) proved smix works on its own. This proves the
# other half: that a session can go from nothing to driving without anyone
# opening a second terminal. The two are judged separately on purpose —
# either one propping up the other would let a gap hide.
#
# What is judged is the tool calls that actually happened, read out of
# `--output-format stream-json`. A session's prose about what it did is
# not evidence: it can describe a successful run it did not have.
#
# The prompt states a goal and not a sequence. Handing over the commands
# would test whether Claude Code can follow a list, when the question is
# whether the skills and tools make the loop findable.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PLUGIN="$ROOT/plugin"
APP="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
BUNDLE="jp.golia.smix.fixture"

log()  { printf '[c8-loop] %s\n' "$*"; }
step() { printf '[c8-loop] --- %s\n' "$*"; }
fail() { printf '[c8-loop] FAIL: %s\n' "$*" >&2; exit 1; }

# A `claude` session that cannot start says nothing about the plugin. Usage
# limits, an expired login, a missing binary — all of them mean "not
# runnable here", and reporting FAIL tells whoever reads the suite next
# that smix is broken. The distinction matters because this file's real
# assertions are about what a session observes, so a session that never
# ran has produced no evidence either way.
UNRUNNABLE='reached your .* limit|/usage-credits|not logged in|Invalid API key|command not found|credit balance'
skip() { printf '[c8-loop] %s\n' "$*" >&2; printf '%s\n' "C8-PLUGIN-LOOP-SKIP"; exit 0; }
session_unrunnable() { grep -qiE "$UNRUNNABLE" "$1" 2>/dev/null; }


command -v claude >/dev/null || fail "the claude CLI is not on PATH"

log "guard: no batch owner on this machine (yield, never seize)"
pgrep -f 'runner.ts|smix run|supervise' >/dev/null && skip "batch owner active — yielding"

# Its own variable first, then the one the whole tier is driven by.
#
# Without the second, `device-e2e-tier.sh` — which sets SMIX_E2E_UDID and
# nothing else — skips this for want of a name. That reads in the summary
# as "nothing to see" rather than "nobody told it where", and a skipped
# script proves nothing while the gate stays green.
UDID="${SMIX_C8_SIM:-${SMIX_E2E_UDID:-}}"
if [ -z "$UDID" ]; then
  UDID="$(bash "$ROOT/scripts/dev/pick-dev-sim.sh" 2>/dev/null || true)"
fi
[ -n "$UDID" ] || skip "set SMIX_C8_SIM to a UDID (or boot a dev sim)"
log "device: $UDID"

step "build and install the app under test"
bash "$ROOT/scripts/dev/build-fixture-app.sh" >/dev/null 2>&1 || fail "fixture build failed"
xcrun simctl boot "$UDID" >/dev/null 2>&1 || true
xcrun simctl install "$UDID" "$APP" >/dev/null 2>&1 || fail "could not install the fixture"

# The session runs here: an app repo, not this one. Nothing in it knows
# about smix except the plugin.
WORK="$(mktemp -d)"
cleanup() {
  step "teardown"
  "$ROOT/target/release/smix" runner down >/dev/null 2>&1 || true
  cp "$WORK/stream.jsonl" /tmp/c8-stream.jsonl 2>/dev/null || true
  rm -rf "$WORK"
}
trap cleanup EXIT
printf 'A demo app. Built at ./SmixFixture.app\n' > "$WORK/README.md"
cp -R "$APP" "$WORK/SmixFixture.app"

# The smix binaries are on PATH the way an installed copy would be; the
# plugin names them without a path.
export PATH="$ROOT/target/release:$PATH"

step "one session, goal stated, no commands given"
PROMPT="This project is an iOS app whose bundle id is $BUNDLE and which is already installed on a simulator. Using the smix tools available to you:
1. drive it on simulator $UDID,
2. type the word hello into its text field and press its Submit button,
3. confirm from the screen that the app reacted to the tap,
4. then let the device go.
Do not run any shell commands to start or stop a runner — use the tools."

( cd "$WORK" && claude --plugin-dir "$PLUGIN" \
    --tools "Bash" \
    --output-format stream-json --verbose \
    -p "$PROMPT" > "$WORK/stream.jsonl" 2>"$WORK/err.log" ) || true

[ -s "$WORK/stream.jsonl" ] || { tail -10 "$WORK/err.log" >&2; fail "the session produced no stream"; }

# --- judge the tool calls ------------------------------------------------

step "read what the session actually called"
python3 - "$WORK/stream.jsonl" > "$WORK/calls.txt" <<'PY'
import json, sys

calls = []
for line in open(sys.argv[1]):
    line = line.strip()
    if not line:
        continue
    try:
        msg = json.loads(line)
    except json.JSONDecodeError:
        continue
    # Tool uses appear as content blocks on assistant messages.
    content = msg.get("message", {}).get("content")
    if not isinstance(content, list):
        continue
    for block in content:
        if isinstance(block, dict) and block.get("type") == "tool_use":
            name = block.get("name", "")
            arg = block.get("input", {})
            detail = arg.get("command", "") if name == "Bash" else ""
            calls.append(f"{name}\t{detail}")
print("\n".join(calls))
PY

CALLS="$(cat "$WORK/calls.txt")"
[ -n "$CALLS" ] || { head -5 "$WORK/stream.jsonl" >&2; fail "no tool calls found in the stream"; }
log "$(printf '%s\n' "$CALLS" | wc -l | tr -d ' ') tool call(s)"

# Match on the base name. A plugin's MCP tools arrive namespaced —
# `mcp__plugin_<plugin>_<server>__<tool>` — and pinning the full prefix
# would make this test fail the day the namespace format changes, while
# telling us the loop was broken.
named() { printf '%s\n' "$CALLS" | grep -qE "(^|__)$1(\t|$)"; }
any_of() {
  for n in "$@"; do
    named "$n" && return 0
  done
  return 1
}

named "smix_use" \
  || fail "the session never bound a device; calls were:
$CALLS"
log "bound a device through the tool"

any_of "smix_describe" "smix_tree" "smix_find" \
  || fail "the session never sensed the screen; calls were:
$CALLS"
log "sensed the screen"

any_of "smix_fill" "smix_tap" \
  || fail "the session never acted on the app; calls were:
$CALLS"
log "acted on the app"

named "smix_release" \
  || fail "the session never let the device go; calls were:
$CALLS"
log "released the device"

# The whole point: nobody opened a second terminal. A Bash call that
# starts a runner means the loop needed a hand it is not supposed to need.
if printf '%s\n' "$CALLS" | grep -E '^Bash' | grep -qE 'capsule up|runner up|smix run '; then
  fail "the session fell back to the shell to bring a runner up:
$(printf '%s\n' "$CALLS" | grep '^Bash')"
fi
log "no shell fallback to start a runner"

# --- the world afterwards ------------------------------------------------

if pgrep -f "xcodebuild.*SmixRunner" >/dev/null; then
  fail "a runner survived the session"
fi
log "no runner left behind"

log "C8-PLUGIN-LOOP-PASS"
