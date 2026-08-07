#!/usr/bin/env bash
# v2.3-C8 recording e2e: a recording that outlives the command that
# started it, and a file that is still playable afterwards.
#
# `simctl io recordVideo` writes an mp4's trailer (the `moov` atom) when
# it receives SIGINT, and at no other time. Before the ledger, the only
# handle on that child was a struct inside whichever process called
# start_recording — so when that process ended, nothing left alive knew
# there was a SIGINT owed, and the file on disk was unplayable.
#
# What is judged here is therefore not "did a file appear" but "is the
# file playable", checked by looking for the `moov` atom directly. A
# recording torn down by SIGKILL produces a file that passes a size check
# and fails this one, which is exactly the difference that matters.
#
# Two closes are exercised, because both happen in practice:
#   - `smix record stop` from a different process than the one that started it
#   - `smix lease reconcile`, for a session nobody came back for
#
# Device work pins an explicit UDID throughout.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
ALIAS="${SMIX_E2E_DEVICE:-sim-smix-02}"
OUTDIR="$(mktemp -d)"

log()  { printf '[c8-recording] %s\n' "$*"; }
step() { printf '[c8-recording] --- %s\n' "$*"; }
fail() { printf '[c8-recording] FAIL: %s\n' "$*" >&2; exit 1; }

# Is this a playable mp4? Walk the top-level box list and look for `moov`.
# A file cut short by SIGKILL has `ftyp` and `mdat` and no `moov`.
playable() {
  python3 - "$1" <<'PY'
import struct, sys
path = sys.argv[1]
found = set()
with open(path, "rb") as f:
    while True:
        head = f.read(8)
        if len(head) < 8:
            break
        size, kind = struct.unpack(">I4s", head)
        kind = kind.decode("latin1")
        found.add(kind)
        if size == 0:
            break
        if size == 1:
            size = struct.unpack(">Q", f.read(8))[0] - 8
        else:
            size -= 8
        if size < 0:
            break
        f.seek(size, 1)
print(" ".join(sorted(found)))
sys.exit(0 if "moov" in found else 1)
PY
}

cd "$ROOT"
[ -x "$SMIX" ] || fail "no smix binary at $SMIX"

step "0. resolve the device, and refuse to run next to somebody's session"
UDID="$("$SMIX" sim list 2>/dev/null | awk -v a="$ALIAS" '$2 == a || $0 ~ a {print $1; exit}')"
[ -n "$UDID" ] || fail "alias $ALIAS is not registered"
log "device $ALIAS = $UDID"
if pgrep -f "xcodebuild.*id=$UDID" >/dev/null 2>&1; then
  fail "a runner is already driving $UDID — this test would tear down someone's work"
fi

cleanup() {
  "$SMIX" record stop "$UDID" >/dev/null 2>&1 || true
  "$SMIX" lease reconcile "$UDID" >/dev/null 2>&1 || true
  "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true
  rm -rf "$OUTDIR"
}
trap cleanup EXIT

step "1. bring the device up, and prove its screen actually renders"
"$SMIX" sim boot "$UDID" >/dev/null 2>&1 || fail "sim boot failed"
# A device can answer "Booted" while CoreSimulator is still bringing the
# render surfaces up. In that state `recordVideo` reports success and
# writes a zero-byte file — which, without this probe, this script would
# report as "the trailer was never written" and blame the mechanism under
# test. A screenshot is the cheapest proof that there is a screen to record.
if ! xcrun simctl io "$UDID" screenshot "$OUTDIR/probe.png" >/dev/null 2>&1; then
  fail "device $UDID has no render surfaces yet (screenshot failed) — nothing can be recorded from it; \
try \`xcrun simctl shutdown $UDID && xcrun simctl boot $UDID && xcrun simctl bootstatus $UDID -b\`"
fi
log "screen renders — recording has something to capture"
OUT="$("$SMIX" record status "$UDID" 2>/dev/null | grep "^$UDID:")"
echo "$OUT" | grep -q "not recording" || fail "unexpected status: $OUT"

step "2. start recording — the row outlives the command that wrote it"
FIRST="$OUTDIR/first.mov"
"$SMIX" record start "$UDID" --output "$FIRST" >/dev/null 2>&1 \
  || fail "record start failed"
# The command has already exited. If the handle were still only in its
# memory, everything below would be impossible.
LEDGER=".smix/leases/$UDID.json"
[ -f "$LEDGER" ] || fail "no ledger after record start"
python3 -c "
import json,sys
rs=[r for r in json.load(open('$LEDGER'))['resources'] if r['kind']=='recording']
if not rs: sys.exit('no recording row')
if rs[0]['path'] != '$FIRST': sys.exit('row names the wrong path: '+rs[0]['path'])
if not rs[0]['proc']['startedAt']: sys.exit('no start time to verify the pid against')
" || fail "recording row is not usable"

step "2b. a second recording on the same device is refused, by name"
# Device recording is mutually exclusive — `simctl` would say so with a
# host-level error about a mutex. Saying it here, in terms of who holds
# the device, is the difference between a message someone can act on and
# one they have to go looking up.
set +e
OUT="$("$SMIX" record start "$UDID" --output "$OUTDIR/second-attempt.mov" 2>&1)"
RC=$?
set -e
[ "$RC" -ne 0 ] || fail "a second recording was allowed to start"
echo "$OUT" | grep -q "in use by pid" || fail "refusal does not name the holder: $OUT"
log "second recording refused"

step "3. status answers where it is writing, not just whether"
OUT="$("$SMIX" record status "$UDID" 2>/dev/null | grep "^$UDID:")"
log "$OUT"
echo "$OUT" | grep -q "recording to $FIRST" || fail "status does not name the file: $OUT"

# Give it something to record. A still screen still produces a valid
# file, but a moving one makes the check mean more.
xcrun simctl launch "$UDID" com.apple.Preferences >/dev/null 2>&1 || true
sleep 3
xcrun simctl terminate "$UDID" com.apple.Preferences >/dev/null 2>&1 || true

step "4. stop from a different process than the one that started it"
OUT="$("$SMIX" record stop "$UDID" 2>&1)" || fail "record stop failed: $OUT"
echo "$OUT" | grep -q "stopped" || fail "stop did not report stopping: $OUT"
[ -f "$FIRST" ] || fail "no file at $FIRST"
BOXES="$(playable "$FIRST")" || fail "recording is not playable — boxes: $BOXES (no moov: the trailer was never written)"
log "playable, boxes: $BOXES"

step "5. and the ledger no longer claims a recording"
OUT="$("$SMIX" record status "$UDID" 2>/dev/null | grep "^$UDID:")"
echo "$OUT" | grep -q "not recording" || fail "row survived the stop: $OUT"

step "6. a deliberately-started recording is not something reconcile ends"
SECOND="$OUTDIR/second.mov"
"$SMIX" record start "$UDID" --output "$SECOND" >/dev/null 2>&1 \
  || fail "second record start failed"
sleep 2
OUT="$("$SMIX" lease reconcile "$UDID" 2>&1)"
echo "$OUT" | grep -q "not touching it" \
  || fail "reconcile ended a recording somebody asked for: $OUT"
log "left alone, as a live session should be"

step "6b. a recording whose session was killed IS closed, with its trailer"
# The case the ledger exists for: something long-lived started a recording
# and was killed without a chance to stop it. The recording keeps writing
# into a file that will never be playable unless somebody who knows about
# it sends the SIGINT. Simulated by pointing the holder at a dead pid,
# which is the state a `kill -9` leaves behind.
python3 -c "
import json
led = json.load(open('$LEDGER'))
led['holder'] = {'pid': 0, 'startedAt': 'Thu Aug  6 10:00:00 2026', 'cmd': 'smix run killed.yaml'}
json.dump(led, open('$LEDGER', 'w'))
" || fail "could not stage the killed-session state"
OUT="$("$SMIX" lease reconcile "$UDID" 2>&1)"
echo "$OUT" | grep -q "recording stopped" \
  || fail "reconcile did not close an orphaned recording: $OUT"
BOXES="$(playable "$SECOND")" || fail "orphaned recording is not playable — boxes: $BOXES"
log "orphaned recording closed and playable, boxes: $BOXES"

step "7. nothing left behind"
OUT="$("$SMIX" record status "$UDID" 2>/dev/null | grep "^$UDID:")"
echo "$OUT" | grep -q "not recording" || fail "row survived reconcile: $OUT"
pgrep -f "simctl io.*$UDID.*recordVideo" >/dev/null 2>&1 \
  && fail "a recordVideo child is still running"

echo "C8-RECORDING-RECONCILE-PASS"
