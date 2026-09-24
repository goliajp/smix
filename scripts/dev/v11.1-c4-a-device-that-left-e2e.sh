#!/usr/bin/env bash
# A device that leaves while a ledger describes it is kept as a fact.
#
# The user saw an Android emulator exit abnormally three or four times in
# one week and could not say whose it was or when. Nothing on the machine
# could either: no crash report, an empty crash database, ledgers still
# describing the devices — and, for an emulator smix itself had started,
# a console sent to /dev/null.
#
# The subject is built, not waited for:
#   1. our sim-smix-android-01 is started on the port sim-smix-android-02
#      was registered on, so that slot answers for a different AVD;
#   2. `smix sim boot sim-smix-android-02` has to start it somewhere else
#      (it used to refuse), and the ledger has to say which AVD it is and
#      where its console is;
#   3. that emulator is then killed from outside smix, and the next smix
#      command has to notice and keep the departure, console included;
#   4. `lease prune --device` clears that one ledger and no other.
#
# Both emulators are this machine's own AVDs. Exit 0 judged and passed,
# 1 judged and failed, 2 could not judge.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/deadline.sh"

BLOCKER_AVD="sim-smix-android-01"
ALIAS="sim-smix-android-02"
SDK="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}}"

log()  { printf '[c4-left] %s\n' "$*" >&2; }
fail() { printf '[c4-left] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[c4-left] CANNOT JUDGE: %s\n' "$*" >&2; exit 2; }

BLOCKER_SERIAL="" BLOCKER_PID="" SERIAL="" WORK="$(mktemp -d)"
cleanup() {
  if [ -n "$SERIAL" ] && adb devices | grep -q "^$SERIAL[[:space:]]"; then
    with_deadline 20 adb -s "$SERIAL" emu kill >/dev/null 2>&1 || true
  fi
  if [ -n "$BLOCKER_SERIAL" ]; then
    with_deadline 20 adb -s "$BLOCKER_SERIAL" emu kill >/dev/null 2>&1 || true
  fi
  [ -n "$BLOCKER_PID" ] && kill "$BLOCKER_PID" 2>/dev/null
  rm -rf "$WORK"
}
trap cleanup EXIT

command -v adb >/dev/null 2>&1 || cannot_judge "no adb on PATH"
[ -x "$SDK/emulator/emulator" ] || cannot_judge "no emulator binary under $SDK"
for avd in "$BLOCKER_AVD" "$ALIAS"; do
  [ -d "$HOME/.android/avd/$avd.avd" ] || cannot_judge "the AVD $avd does not exist on this machine"
done

registered_port() {
  "$SMIX" sim list --registered 2>/dev/null | grep -v '^kevy:' \
    | awk -v a="$1" '$1 == a { sub("emulator-", "", $2); print $2 }'
}
PORT2="$(registered_port "$ALIAS")"
[ -n "$PORT2" ] || cannot_judge "$ALIAS is not registered"
BLOCKER_SERIAL="emulator-$PORT2"

# Never take a port somebody else is on. If the slot is already answering,
# this cannot build its subject without taking theirs.
if adb devices | grep -q "^$BLOCKER_SERIAL[[:space:]]"; then
  cannot_judge "$BLOCKER_SERIAL is already answering for something; this builds its subject by occupying that port with our own AVD"
fi

wait_for_boot() {
  local serial="$1" i
  for i in $(seq 1 60); do
    [ "$(with_deadline 5 adb -s "$serial" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ] && return 0
    sleep 3
  done
  return 1
}

step() { log "--- $*"; }

step "occupy $ALIAS's registered port ($PORT2) with $BLOCKER_AVD"
"$SDK/emulator/emulator" -avd "$BLOCKER_AVD" -port "$PORT2" -no-snapshot-save -no-boot-anim \
  >"$WORK/blocker.log" 2>&1 &
BLOCKER_PID=$!
wait_for_boot "$BLOCKER_SERIAL" || cannot_judge "$BLOCKER_AVD did not come up on $BLOCKER_SERIAL — see $WORK/blocker.log"
log "$BLOCKER_SERIAL is $BLOCKER_AVD"

step "smix sim boot $ALIAS — its port is taken, so it has to go elsewhere"
out="$(with_deadline 240 "$SMIX" sim boot "$ALIAS" 2>&1 | grep -v '^kevy:')"
rc=$?
[ "$rc" = "$DEADLINE_STATUS" ] && cannot_judge "sim boot did not return in 240 s"
[ "$rc" = 0 ] || fail "sim boot refused or failed with its registered port taken (exit $rc):
$out"
SERIAL="$(printf '%s\n' "$out" | sed -n 's/^booted: \(emulator-[0-9]*\).*/\1/p' | tail -1)"
[ -n "$SERIAL" ] || fail "sim boot printed no \`booted:\` line:
$out"
[ "$SERIAL" != "$BLOCKER_SERIAL" ] || fail "sim boot reported the occupied port $SERIAL"
[ "$(with_deadline 10 adb -s "$SERIAL" emu avd name 2>/dev/null | head -1 | tr -d '\r')" = "$ALIAS" ] \
  || fail "$SERIAL is not $ALIAS"
log "boot-elsewhere=yes ($ALIAS on $SERIAL, $BLOCKER_SERIAL left to $BLOCKER_AVD)"

LEDGER="$HOME/.local/share/smix/leases/$SERIAL.json"
[ -f "$LEDGER" ] || fail "no ledger at $LEDGER after smix booted $SERIAL"
CONSOLE="$(LEDGER="$LEDGER" python3 - <<'PY'
import json, os
d = json.load(open(os.environ["LEDGER"]))
rows = [r for r in d.get("resources", []) if r.get("kind") == "emulator"]
print(rows[0].get("consoleLog", "") if rows and rows[0].get("avd") else "")
PY
)"
[ -n "$CONSOLE" ] || fail "the ledger for $SERIAL does not say which AVD it is or where its console goes"
[ -s "$CONSOLE" ] || fail "the console log $CONSOLE is empty — the emulator's output is still being thrown away"
log "ledger-names-avd=yes console=$CONSOLE ($(wc -l <"$CONSOLE" | tr -d ' ') lines so far)"

step "kill $SERIAL from outside smix"
with_deadline 20 adb -s "$SERIAL" emu kill >/dev/null 2>&1 || true
for _ in $(seq 1 30); do
  adb devices | grep -q "^$SERIAL[[:space:]]" || break
  sleep 1
done
adb devices | grep -q "^$SERIAL[[:space:]]" && cannot_judge "$SERIAL was still listed 30 s after emu kill"

step "the next smix command notices"
said="$(with_deadline 60 "$SMIX" lease list 2>&1 >/dev/null | grep -v '^kevy:')"
printf '%s\n' "$said" | grep -q "$SERIAL.*left without smix hearing about it" \
  || fail "\`smix lease list\` did not say $SERIAL left:
$said"
log "noticed=yes"

HIST="$(with_deadline 30 "$SMIX" lease history --json 2>/dev/null | grep -v '^kevy:')"
SERIAL="$SERIAL" ALIAS="$ALIAS" HIST="$HIST" python3 - <<'PY' || exit 1
import json, os, sys
serial, alias = os.environ["SERIAL"], os.environ["ALIAS"]
entries = [e for e in json.loads(os.environ["HIST"]) if e["deviceId"] == serial]
def bad(msg):
    print(f"[c4-left] FAIL: {msg}", file=sys.stderr)
    sys.exit(1)
if len(entries) != 1:
    bad(f"history has {len(entries)} entries for {serial}, not exactly one")
e = entries[0]
if e.get("avd") != alias:
    bad(f"the departure names AVD {e.get('avd')!r}, not {alias!r}")
if not e.get("bootedBySmix"):
    bad("the departure does not say smix booted it")
if not e.get("consoleLog") or not e.get("consoleTail"):
    bad("the departure carries no console — what the emulator last said is lost")
print(f"[c4-left]   history: avd={e['avd']} booted_by_smix=yes last_heard={e['lastHeartbeat']} console_tail={len(e['consoleTail'])} lines", file=sys.stderr)
PY

step "lease prune --device $SERIAL clears that ledger and no other"
before="$(ls "$HOME/.local/share/smix/leases/" | grep -c '\.json$')"
out="$(with_deadline 60 "$SMIX" lease prune --device "$SERIAL" 2>&1 | grep -v '^kevy:')"
[ -f "$LEDGER" ] && fail "prune --device $SERIAL kept a ledger whose device is gone:
$out"
after="$(ls "$HOME/.local/share/smix/leases/" | grep -c '\.json$')"
[ "$after" = "$((before - 1))" ] || fail "prune --device removed $((before - after)) ledgers, not exactly one"
log "pruned-one=yes"

log "C4-LEFT-E2E-PASS ($ALIAS booted beside a taken port, left, was noticed with its console, and was pruned alone)"
