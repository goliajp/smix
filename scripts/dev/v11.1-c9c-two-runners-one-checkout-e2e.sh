#!/usr/bin/env bash
# Two iOS runners brought up from one checkout keep one record each.
#
# A consumer drove two apps from one checkout, each on its own simulator
# and port, and lost a round of 40 flows to it. The iOS runner record was
# one slot per platform per checkout: the second `runner up` overwrote the
# first's, `runner down` on the second cleared the slot, and `runner up
# --force` on the first then refused its own runner — "already serves
# /health but the store has no record of that runner — not killing
# blindly". The same slot made `runner down --runner-port A` stop B when B
# was the one brought up last: it read the slot, not the port.
#
# The consumer's four steps, then the same question from the other side:
#   1. runner up on A (port PA)
#   2. runner up on B (port PB)
#   3. runner down --runner-port PB  → B stops; A still answers, its row stays
#   4. runner up A --force on PA     → recognises A (up, or cycled in place)
#   5. runner up on B again
#   6. runner down --runner-port PA  → A stops, and only A: B still answers
#
# Judged by what the ports answer and what the device ledger holds, never
# by what smix prints about itself — except step 4, where the sentence it
# prints is the defect.
#
# Exit 0 judged and passed, 1 judged and failed, 2 could not judge.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/deadline.sh"
source "$ROOT/scripts/lib/gate-port.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
gate_free_port PA
gate_free_port PB

A_REF="${SMIX_C9C_IOS_A:-$E2E_IOS}"
B_UDID="${SMIX_C9C_IOS_B:-$E2E_IOS_SECOND}"
BUNDLE="com.apple.Preferences"
MACHINE="${SMIX_MACHINE_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/smix}"

log()  { printf '[c9c-two-runners] %s\n' "$*" >&2; }
step() { log "--- $*"; }
fail() { printf '[c9c-two-runners] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[c9c-two-runners] CANNOT JUDGE: %s\n' "$*" >&2; exit 2; }

A_UDID="" A_WE_BOOTED=0 B_WE_BOOTED=0 WORK="$(mktemp -d)"
cleanup() {
  local p
  for p in "$PA" "$PB"; do
    healthy "$p" || continue
    # Both ports were handed to this script by the OS; whatever answers on
    # them is a runner this script started. Without the ledger naming it
    # (a build with the defect under test), only consent reaches it.
    (cd "$ROOT" && with_deadline 90 "$SMIX" runner down --runner-port "$p") >/dev/null 2>&1
    healthy "$p" && (cd "$ROOT" && with_deadline 90 "$SMIX" runner down --runner-port "$p" \
      --include-unrecorded) >/dev/null 2>&1
    healthy "$p" && log "warning: the runner on port $p was not stopped"
  done
  if [ "$A_WE_BOOTED" = 1 ]; then with_deadline 60 "$SMIX" sim shutdown "$A_UDID" >/dev/null 2>&1 || true; fi
  if [ "$B_WE_BOOTED" = 1 ]; then with_deadline 60 "$SMIX" sim shutdown "$B_UDID" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

healthy() { # healthy <port> — the runner's own /health answers 200
  [ "$(curl -s -m 3 -o /dev/null -w '%{http_code}' "http://127.0.0.1:$1/health" 2>/dev/null)" = 200 ]
}
goes_quiet() { # goes_quiet <port> <seconds>
  local i
  for i in $(seq 1 "$2"); do healthy "$1" || return 0; sleep 1; done
  return 1
}
ledger_runner_port() { # the port the device ledger records a runner on, or nothing
  python3 -c '
import json, os, sys
path = os.path.join(sys.argv[1], "leases", sys.argv[2] + ".json")
try:
    lease = json.load(open(path))
except FileNotFoundError:
    sys.exit(0)
for r in lease.get("resources", []):
    if r.get("kind") == "runner":
        print(r["port"])
' "$MACHINE" "$1"
}
holder_is_other() { # a live holder that is not smix run by this script
  python3 -c '
import json, os, sys
path = os.path.join(sys.argv[1], "leases", sys.argv[2] + ".json")
try:
    lease = json.load(open(path))
except FileNotFoundError:
    sys.exit(1)
pid = lease.get("holder", {}).get("pid")
try:
    os.kill(pid, 0)
except (OSError, TypeError):
    sys.exit(1)
print(lease["holder"].get("cmd", ""))
' "$MACHINE" "$1"
}
sim_state() { # sim_state <udid> — simctl's state and name, from its own list
  xcrun simctl list devices -j | python3 -c 'import json,sys
u=sys.argv[1]
for ds in json.load(sys.stdin)["devices"].values():
    for d in ds:
        if d["udid"] == u: print(d["state"], d["name"])' "$1"
}
up() { # up <udid> <port> [flags...] — output to $WORK/up.out, status to $UP_RC
  local udid="$1" port="$2"
  shift 2
  UP_RC=0
  (cd "$ROOT" && with_deadline 900 "$SMIX" runner up "$udid" --bundle "$BUNDLE" \
    --runner-port "$port" "$@") >"$WORK/up.out" 2>&1 || UP_RC=$?
  cat "$WORK/up.out" >&2 || true
  [ "$UP_RC" = "$DEADLINE_STATUS" ] && cannot_judge "runner up on $udid did not return in 900 s"
  return 0
}
down() { # down <port> — output to $WORK/down.out, status to $DOWN_RC
  DOWN_RC=0
  (cd "$ROOT" && with_deadline 120 "$SMIX" runner down --runner-port "$1") \
    >"$WORK/down.out" 2>&1 || DOWN_RC=$?
  cat "$WORK/down.out" >&2 || true
  [ "$DOWN_RC" = "$DEADLINE_STATUS" ] && cannot_judge "runner down on port $1 did not return in 120 s"
  return 0
}

# --- the two simulators, and that they are ours to drive ------------------
A_UDID="$("$SMIX" sim resolve "$A_REF" 2>/dev/null | tail -1)"
[ -n "$A_UDID" ] || cannot_judge "no iOS simulator registered as $A_REF"
[ "$A_UDID" != "$B_UDID" ] || cannot_judge "A and B are the same simulator ($A_UDID)"
for u in "$A_UDID" "$B_UDID"; do
  st="$(sim_state "$u")"
  [ -n "$st" ] || cannot_judge "simctl knows no simulator $u"
  case "$st" in *" sim-smix-"*) ;; *) cannot_judge "$u is '$st', not one of smix's own simulators" ;; esac
  if who="$(holder_is_other "$u")"; then
    cannot_judge "$u is held by a live session ($who) — not ours to drive"
  fi
done
for p in "$PA" "$PB"; do healthy "$p" && cannot_judge "port $p already answers — another runner is there"; done

needs_boot() { case "$(sim_state "$1")" in Booted*) return 1 ;; *) return 0 ;; esac; }
boot() {
  with_deadline 300 "$SMIX" sim boot "$1" >"$WORK/boot.log" 2>&1 \
    || cannot_judge "could not boot $1: $(tail -3 "$WORK/boot.log" | tr '\n' ' ')"
}
if needs_boot "$A_UDID"; then boot "$A_UDID"; A_WE_BOOTED=1; fi
if needs_boot "$B_UDID"; then boot "$B_UDID"; B_WE_BOOTED=1; fi
log "A=$A_UDID port $PA · B=$B_UDID port $PB"

step "1. runner up on A"
up "$A_UDID" "$PA"
[ "$UP_RC" = 0 ] && healthy "$PA" || cannot_judge "1: the runner on A did not come up — nothing below could be judged"
[ "$(ledger_runner_port "$A_UDID")" = "$PA" ] \
  || cannot_judge "1: A's ledger records no runner on $PA — the subject was not built"

step "2. runner up on B"
up "$B_UDID" "$PB"
[ "$UP_RC" = 0 ] && healthy "$PB" || cannot_judge "2: the runner on B did not come up — nothing below could be judged"
[ "$(ledger_runner_port "$B_UDID")" = "$PB" ] \
  || cannot_judge "2: B's ledger records no runner on $PB — the subject was not built"

step "3. runner down --runner-port $PB"
down "$PB"
[ "$DOWN_RC" = 0 ] || fail "3: runner down on B's port exited $DOWN_RC"
goes_quiet "$PB" 30 || fail "3: B's runner still answers on $PB after runner down"
healthy "$PA" || fail "3: taking B down stopped A's runner on $PA"
[ "$(ledger_runner_port "$A_UDID")" = "$PA" ] || fail "3: taking B down erased A's runner row"
log "3=B-stopped, A answering and recorded"

step "4. runner up A --force on $PA (the consumer's step 4)"
up "$A_UDID" "$PA" --force
if grep -q "not killing blindly" "$WORK/up.out"; then
  fail "4: runner up --force refused A's own runner as unrecorded — the consumer's report, reproduced"
fi
[ "$UP_RC" = 0 ] || fail "4: runner up --force on A exited $UP_RC"
grep -qE "runner already up: udid=$A_UDID|cycling it in place" "$WORK/up.out" \
  || fail "4: runner up --force answered without recognising A's runner"
healthy "$PA" || fail "4: A's runner does not answer after runner up --force"
log "4=recognised ($(grep -m1 -oE 'runner already up|cycling it in place' "$WORK/up.out"))"

step "5. runner up on B again"
up "$B_UDID" "$PB"
[ "$UP_RC" = 0 ] && healthy "$PB" || cannot_judge "5: the runner on B did not come back up"

step "6. runner down --runner-port $PA (B was brought up last)"
down "$PA"
# The runner it stopped before its exit status: a down that stops B and
# then refuses A exits 1, and "exited 1" would hide which runner it took.
if grep -q "udid=$B_UDID" "$WORK/down.out"; then
  fail "6: runner down --runner-port $PA stopped B's runner — it read the last one brought up, not the port"
fi
[ "$DOWN_RC" = 0 ] || fail "6: runner down on A's port exited $DOWN_RC"
goes_quiet "$PA" 30 || fail "6: A's runner still answers on $PA after runner down"
healthy "$PB" || fail "6: taking A down stopped B's runner on $PB"
[ "$(ledger_runner_port "$B_UDID")" = "$PB" ] || fail "6: taking A down erased B's runner row"
[ -z "$(ledger_runner_port "$A_UDID")" ] || fail "6: A's runner row outlived its runner"
log "6=A-stopped, B answering and recorded"

log "C9C-TWO-RUNNERS-E2E-PASS on $A_UDID and $B_UDID"
