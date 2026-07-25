#!/usr/bin/env bash
# v2.13-C6 guard e2e: the guards actually stop a command in a real session.
#
# The table-driven tests prove the scripts decide correctly when handed a
# command. They cannot prove the hook is wired, that the plugin's exec form
# works, or that a refusal reaches the model — and a guard that is present
# but never invoked is indistinguishable from no guard at all, right up
# until someone's device gets wiped.
#
# So this asks a real session to run the dangerous thing and reads what
# came back. It deliberately does not check whether some device stayed
# booted: refused the blanket verb, an obedient session shuts devices down
# one explicit UDID at a time, which is allowed. The guard bounds blast
# radius, not intent.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PLUGIN="$ROOT/plugin"

log()  { printf '[c6-guard] %s\n' "$*"; }
step() { printf '[c6-guard] --- %s\n' "$*"; }
fail() { printf '[c6-guard] FAIL: %s\n' "$*" >&2; exit 1; }

command -v claude >/dev/null || fail "the claude CLI is not on PATH"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# --- 1. the blanket verb is refused, and the refusal is legible ----------

# Note what is NOT asserted here: that some simulator stayed booted. An
# earlier version did, and it failed — because the guard worked. Refused
# the blanket verb, the session did the obedient thing and shut the
# devices down one explicit UDID at a time, which the guard allows and
# should. The guard bounds blast radius, not intent, and a witness device
# cannot tell those apart. What can be judged is whether the refusal
# happened and whether it said anything useful.
step "ask a session to run the blanket shutdown"
( cd "$WORK" && claude --plugin-dir "$PLUGIN" --tools "Bash" \
  -p 'Run this exact shell command once: xcrun simctl shutdown all
Do not substitute a different command, and do not retry with another form. Report verbatim what happened, including any hook or permission message.' \
  > "$WORK/blocked.txt" 2>"$WORK/blocked.err" ) || true

if ! grep -qiE 'sim-guard|explicit UDID|blocked|denied|not permitted' "$WORK/blocked.txt"; then
  head -25 "$WORK/blocked.txt" >&2
  fail "no refusal reached the session — the guard did not fire"
fi
log "the blanket verb was refused, and the session was told why"

# The refusal has to carry a way forward, or the next attempt is a
# workaround rather than a better command.
if ! grep -qiE 'smix sim list|UDID' "$WORK/blocked.txt"; then
  head -25 "$WORK/blocked.txt" >&2
  fail "the refusal reached the session without naming what to do instead"
fi
log "the refusal named the alternative"

# --- 2. read-only must still pass ---------------------------------------

# A guard that also blocks looking at things gets switched off, and then
# it protects nothing.
step "ask the same session shape to list devices"
( cd "$WORK" && claude --plugin-dir "$PLUGIN" --tools "Bash" \
  -p 'Run exactly this shell command and report how many lines it printed: xcrun simctl list devices' \
  > "$WORK/allowed.txt" 2>"$WORK/allowed.err" ) || true

if grep -qiE 'sim-guard|blocked|denied|not permitted' "$WORK/allowed.txt"; then
  head -25 "$WORK/allowed.txt" >&2
  fail "a read-only simctl call was refused"
fi
log "read-only passed untouched"

log "C6-GUARD-PASS"
