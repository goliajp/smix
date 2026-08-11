#!/usr/bin/env bash
# What is running here, next to what is written down.
#
# On 2026-08-11 a runner held port 22087 and the ledger the rule told us
# to consult had no record of it, so it could neither be confirmed an
# orphan nor killed. Neither side answers alone: the ledgers say what
# somebody said they opened, `lsof` says what is actually listening.
#
# Nothing here boots or shuts down a device. The listener in step 5 is a
# python process this script starts, placed under a path shaped like a
# simulator's container — which is the only thing the probe judges on,
# since the process holding a runner's socket is the app inside the
# simulator and its command line is where its device is named. The trap
# kills it.
#
# The load-bearing steps are 5 and 7. Steps 3, 6 and 8 can be satisfied
# by a well-chosen sentence; step 5 needs the probe to exist and to have
# produced that row, and step 7 needs the pairing to have recognised one
# runner as one runner.
#
# Usage: bash scripts/dev/v3.1-c4-runner-view-e2e.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
PASS=0
FAIL=0

step() { echo; echo "=== $* ==="; }
ok()   { echo "  PASS: $*"; PASS=$((PASS + 1)); }
bad()  { echo "  FAIL: $*"; FAIL=$((FAIL + 1)); }
# Captured, then matched. `cmd | grep -q` reads as "not found" when it
# means "grep closed the pipe, the writer took SIGPIPE, and pipefail
# called the pipeline failed".
has()  { case "$1" in *"$2"*) return 0 ;; *) return 1 ;; esac }

step "0. build"
cargo build -p smix-cli --manifest-path "$ROOT/Cargo.toml" >/dev/null 2>&1 \
    || { echo "cannot build smix-cli"; exit 2; }
[ -x "$SMIX" ] || { echo "no smix at $SMIX"; exit 2; }

M="$(mktemp -d)"
W="$(mktemp -d)"
LISTENER_PID=""
cleanup() {
    [ -n "$LISTENER_PID" ] && kill "$LISTENER_PID" 2>/dev/null || true
    rm -rf "$M" "$W"
}
trap cleanup EXIT
mkdir -p "$M/leases" "$W/.smix/leases"

DEV_A="AAAAAAAA-1111-2222-3333-444444444444"
DEV_B="BBBBBBBB-1111-2222-3333-555555555555"

ledger() {  # $1 = dir, $2 = device, $3 = port, $4 = runner pid
    cat > "$1/$2.json" <<JSON
{
  "deviceId": "$2",
  "holder": {"pid": 999001, "startedAt": "Sun Aug  9 07:18:59 2026", "cmd": "smix-mcp"},
  "acquiredAt": "2026-08-09T07:18:59Z",
  "heartbeatAt": "2026-08-11T08:00:55Z",
  "resources": [
    {"kind": "runner", "port": $3,
     "proc": {"pid": $4, "startedAt": "Tue Aug 11 19:54:21 2026", "cmd": "xcodebuild test"}}
  ]
}
JSON
}

step "1. two devices, registered so they can be addressed"
# Not optional. §9 #1 refuses a device nothing has registered, and a
# refusal there looks exactly like the refusal a later step is testing
# for — which is how the previous checkpoint's load-bearing step passed
# twice with its own subject commented out.
for d in "$DEV_A" "$DEV_B"; do
    SMIX_MACHINE_DIR="$M" "$SMIX" sim register "fixture-${d:0:4}" \
        --udid "$d" --kind physical-ios >/dev/null 2>&1 \
        || { echo "cannot register $d"; exit 2; }
done
ok "registered two fixture devices in a throwaway machine directory"

step "2. find a port nothing is on"
FREE_PORT="$(python3 -c 'import socket
s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')"
DEAD_PORT="$(python3 -c 'import socket
s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')"
echo "  listener will take :$FREE_PORT; the ledger-only row claims :$DEAD_PORT"

step "3. a ledger with nothing behind it says so"
ledger "$M/leases" "$DEV_A" "$DEAD_PORT" 999100
OUT="$(SMIX_MACHINE_DIR="$M" "$SMIX" runner list 2>/dev/null || true)"
if has "$OUT" "$DEV_A" && has "$OUT" "ledger-only" && has "$OUT" "999100"; then
    ok "the recorded runner nobody is holding reads as ledger-only"
else
    bad "expected a ledger-only row for $DEV_A: $OUT"
fi

step "4. it reads and writes nothing"
BEFORE="$(shasum -a 256 "$M/leases/$DEV_A.json" | awk '{print $1}')"
set +e
SMIX_MACHINE_DIR="$M" "$SMIX" runner list >/dev/null 2>&1
RC=$?
set -e
AFTER="$(shasum -a 256 "$M/leases/$DEV_A.json" | awk '{print $1}')"
if [ "$RC" = 0 ] && [ "$BEFORE" = "$AFTER" ]; then
    ok "exit 0 and the ledger is byte-identical"
else
    bad "exit $RC, ledger $( [ "$BEFORE" = "$AFTER" ] && echo unchanged || echo CHANGED )"
fi

step "5. LOAD-BEARING — a listener no ledger knows about"
# Placed where a simulator's app would be. The probe reads the device
# out of the holder's command line, so the path is the whole point.
BUNDLE="$W/Devices/$DEV_B/data/Containers/Bundle/Application/SmixRunner.app"
mkdir -p "$BUNDLE"
cat > "$BUNDLE/SmixRunner" <<'PY'
import socket, sys, time
s = socket.socket()
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(("127.0.0.1", int(sys.argv[1])))
s.listen(1)
while True:
    time.sleep(3600)
PY
python3 "$BUNDLE/SmixRunner" "$FREE_PORT" &
LISTENER_PID=$!
for _ in $(seq 1 40); do
    if lsof -nP -t "-iTCP:$FREE_PORT" -sTCP:LISTEN >/dev/null 2>&1; then break; fi
    sleep 0.1
done
OUT="$(SMIX_MACHINE_DIR="$M" "$SMIX" runner list 2>/dev/null || true)"
if has "$OUT" "$DEV_B" && has "$OUT" "process-only" && has "$OUT" "$LISTENER_PID"; then
    ok "a listener nothing recorded is found, named and attributed to $DEV_B"
else
    bad "the probe did not see the listener on :$FREE_PORT (pid $LISTENER_PID): $OUT"
fi

step "6. a tree that has a record of it is named, and called evidence"
ledger "$W/.smix/leases" "$DEV_B" "$FREE_PORT" 999200
OUT="$(cd "$W" && SMIX_MACHINE_DIR="$M" "$SMIX" runner list 2>/dev/null || true)"
if has "$OUT" "$W" && has "$OUT" "evidence"; then
    ok "the checkout is named as evidence, not as authority"
else
    bad "the tree holding a record of $DEV_B went unmentioned: $OUT"
fi

step "7. LOAD-BEARING — one runner is one row, not two"
# The ledger's pid is the xcodebuild session on the host; the socket
# belongs to the app. They are different by construction, so pairing on
# pid would split one live runner into a ledger-only row and a
# process-only row.
ledger "$M/leases" "$DEV_B" "$FREE_PORT" 999300
OUT="$(SMIX_MACHINE_DIR="$M" "$SMIX" runner list 2>/dev/null || true)"
ROWS="$(printf '%s\n' "$OUT" | grep -c ":$FREE_PORT " || true)"
BROWS="$(printf '%s\n' "$OUT" | grep "$DEV_B" | grep -cE "ledger-only|process-only" || true)"
if [ "$ROWS" = 1 ] && [ "$BROWS" = 0 ] && has "$OUT" "both"; then
    ok "one row for (:$FREE_PORT, $DEV_B), and it is neither one-sided"
else
    bad "rows on :$FREE_PORT = $ROWS, one-sided rows for $DEV_B = $BROWS: $OUT"
fi

step "8. with nothing on either side, it says so and names nothing"
E_M="$(mktemp -d)"; E_W="$(mktemp -d)"
OUT="$(cd "$E_W" && SMIX_MACHINE_DIR="$E_M" "$SMIX" runner list 2>/dev/null || true)"
rm -rf "$E_M" "$E_W"
if has "$OUT" "$DEV_A" || has "$OUT" "$DEV_B"; then
    bad "an empty machine directory still named this script's fixtures: $OUT"
else
    ok "an empty machine says so without inventing rows"
fi

echo
echo "=== $PASS passed, $FAIL failed ==="
[ "$FAIL" = 0 ]
