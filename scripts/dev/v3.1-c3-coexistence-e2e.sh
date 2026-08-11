#!/usr/bin/env bash
# Two books about one device, and what smix will not do while they differ.
#
# The ledgers moved to ~/.local/share/smix/leases on 2026-08-11. The copy
# of smix installed on that machine did not: `~/.local/bin/smix` was
# 3.0.0 and still wrote into whichever `.smix/` sat above the working
# directory. Ninety-one minutes after the migration, one checkout's
# ledger had been rewritten, recording a runner that was alive and
# serving on port 22087, while the machine ledger for the same device
# named a pid that had exited and read "abandoned — 2 close(s) owed". A
# `lease reconcile` from any tree, in that window, would have shut the
# simulator down and killed the runner.
#
# Everything here runs against ledgers this script writes, in two
# temporary directories. It boots nothing, shuts nothing down, and never
# names a real UDID: a check that needed some session to still be alive
# would be green tomorrow for a reason nobody could state.
#
# The load-bearing steps are 4 and 8. The others can be satisfied by a
# well-chosen sentence; step 4 requires that the comparison exist and
# actually stop an action (exit non-zero, machine ledger byte-identical
# afterwards), and step 8 requires that the divergence lines come from a
# comparison rather than from a template.
#
# Usage: bash scripts/dev/v3.1-c3-coexistence-e2e.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
PASS=0
FAIL=0

step() { echo; echo "=== $* ==="; }
ok()   { echo "  PASS: $*"; PASS=$((PASS + 1)); }
bad()  { echo "  FAIL: $*"; FAIL=$((FAIL + 1)); }

# Captured, then matched. `cmd | grep -q` reads as "not found" when it
# means "grep closed the pipe on its first match, the writer took
# SIGPIPE, and pipefail called the pipeline failed" — which is what the
# C2 e2e reported one step before another command found the very ledger
# it had just said was missing.
has() { case "$1" in *"$2"*) return 0 ;; *) return 1 ;; esac }

step "0. build"
cargo build -p smix-cli --manifest-path "$ROOT/Cargo.toml" >/dev/null 2>&1 \
    || { echo "cannot build smix-cli"; exit 2; }
[ -x "$SMIX" ] || { echo "no smix at $SMIX"; exit 2; }

M="$(mktemp -d)"
W="$(mktemp -d)"
trap 'rm -rf "$M" "$W"' EXIT
mkdir -p "$M/leases" "$W/.smix/leases"

DEV="FFFFFFFF-1111-2222-3333-444444444444"

# Two ledgers for one device, naming different runners. The machine's
# runner pid is one nothing will ever run at; the tree's is another.
ledger() {  # $1 = path, $2 = runner pid
    cat > "$1/$DEV.json" <<JSON
{
  "deviceId": "$DEV",
  "holder": {"pid": 999001, "startedAt": "Sun Aug  9 07:18:59 2026", "cmd": "smix-mcp"},
  "acquiredAt": "2026-08-09T07:18:59Z",
  "heartbeatAt": "2026-08-11T08:00:55Z",
  "resources": [
    {"kind": "booted", "byUs": true},
    {"kind": "runner", "port": 22087,
     "proc": {"pid": $2, "startedAt": "Tue Aug 11 19:54:21 2026", "cmd": "xcodebuild test"}}
  ]
}
JSON
}

step "1. one device, two books, two runners"
# Registered first, into the throwaway machine directory this script
# made — SMIX_MACHINE_DIR moves the device registry too, so nothing here
# touches the real one. Without it §9 #1 refuses the device before any
# of this is reached, and the refusal below would be indistinguishable
# from "smix has never heard of it": step 4 passed with its own subject
# commented out until this line existed.
SMIX_MACHINE_DIR="$M" "$SMIX" sim register fixture \
    --udid "$DEV" --kind physical-ios >/dev/null 2>&1 \
    || { echo "cannot register the fixture device"; exit 2; }
ledger "$M/leases" 999100
ledger "$W/.smix/leases" 999200
echo "  machine: $M/leases/$DEV.json (runner pid 999100)"
echo "  tree:    $W/.smix/leases/$DEV.json (runner pid 999200)"

step "2. the tree's book is not merged into the answer"
OUT="$(cd "$W" && SMIX_MACHINE_DIR="$M" "$SMIX" lease list 2>/dev/null || true)"
if has "$OUT" "$DEV"; then
    ok "the device is listed"
else
    bad "the machine ledger was not read at all: $OUT"
fi

step "3. the disagreement is said out loud, with both pids"
ERR="$(cd "$W" && SMIX_MACHINE_DIR="$M" "$SMIX" lease list 2>&1 >/dev/null || true)"
if has "$ERR" "999100" && has "$ERR" "999200" && has "$ERR" "$W"; then
    ok "both pids and the tree's path are named"
else
    bad "the note does not let somebody go and look: $ERR"
fi

step "4. LOAD-BEARING — reconcile refuses, and writes nothing"
BEFORE="$(shasum -a 256 "$M/leases/$DEV.json" | awk '{print $1}')"
set +e
( cd "$W" && SMIX_MACHINE_DIR="$M" "$SMIX" lease reconcile "$DEV" >/dev/null 2>&1 )
RC=$?
set -e
AFTER="$(shasum -a 256 "$M/leases/$DEV.json" | awk '{print $1}')"
if [ "$RC" != 0 ] && [ "$BEFORE" = "$AFTER" ]; then
    ok "refused (exit $RC) and the machine ledger is byte-identical"
else
    bad "exit $RC, ledger $( [ "$BEFORE" = "$AFTER" ] && echo unchanged || echo CHANGED ) \
— a settle ran while the two books disagreed"
fi

step "5. books that agree are not a complaint"
ledger "$W/.smix/leases" 999100
ERR="$(cd "$W" && SMIX_MACHINE_DIR="$M" "$SMIX" lease list 2>&1 >/dev/null || true)"
if has "$ERR" "$DEV"; then
    bad "identical books still reported as differing: $ERR"
else
    ok "a source left in place after migrating says nothing"
fi

step "6. a device only the tree has is the completeness answer"
DEV2="FFFFFFFF-1111-2222-3333-555555555555"
DEV_SAVE="$DEV"; DEV="$DEV2"; ledger "$W/.smix/leases" 999300; DEV="$DEV_SAVE"
ERR="$(cd "$W" && SMIX_MACHINE_DIR="$M" "$SMIX" lease list 2>&1 >/dev/null || true)"
COUNT_BEFORE="$(find "$M/leases" -name '*.json' | wc -l | tr -d ' ')"
DRY="$(cd "$W" && SMIX_MACHINE_DIR="$M" "$SMIX" lease migrate --dry-run --from "$W" 2>/dev/null || true)"
COUNT_AFTER="$(find "$M/leases" -name '*.json' | wc -l | tr -d ' ')"
if has "$ERR" "$DEV2" && has "$DRY" "$DEV2" && [ "$COUNT_BEFORE" = "$COUNT_AFTER" ]; then
    ok "named in the report and in the rehearsal, and nothing was written"
else
    bad "list=$ERR / dry=$DRY / files $COUNT_BEFORE→$COUNT_AFTER"
fi

step "7. migrate it, then the rehearsal has nothing left to say"
( cd "$W" && SMIX_MACHINE_DIR="$M" "$SMIX" lease migrate --from "$W" >/dev/null 2>&1 ) || true
DRY="$(cd "$W" && SMIX_MACHINE_DIR="$M" "$SMIX" lease migrate --dry-run --from "$W" 2>/dev/null || true)"
if has "$DRY" "0 ledger(s) would move"; then
    ok "after migrating, the rehearsal reports nothing to move"
else
    bad "the rehearsal still claims work after the real run: $DRY"
fi

step "8. LOAD-BEARING — with nothing to compare against, nothing is said"
EMPTY_M="$(mktemp -d)"; EMPTY_W="$(mktemp -d)"
ERR="$(cd "$EMPTY_W" && SMIX_MACHINE_DIR="$EMPTY_M" "$SMIX" lease list 2>&1 >/dev/null || true)"
rm -rf "$EMPTY_M" "$EMPTY_W"
if has "$ERR" "recorded differently" || has "$ERR" "and none here"; then
    bad "divergence lines printed with no second book to compare against: $ERR"
else
    ok "no second book, no divergence lines — they come from a comparison"
fi

step "9. an empty machine and a tree that still holds one"
# The state a fresh checkout of a mid-migration machine is in, and the
# one where being told matters most. `lease list` returned early on an
# empty machine directory — printing "no device ledgers" while a tree
# held the only record of a device — until this step existed.
E_M="$(mktemp -d)"; E_W="$(mktemp -d)"
mkdir -p "$E_M/leases" "$E_W/.smix/leases"
M_SAVE="$M"; W_SAVE="$W"; M="$E_M"; W="$E_W"
ledger "$E_W/.smix/leases" 999400
M="$M_SAVE"; W="$W_SAVE"
ERR="$(cd "$E_W" && SMIX_MACHINE_DIR="$E_M" "$SMIX" lease list 2>&1 >/dev/null || true)"
rm -rf "$E_M" "$E_W"
if has "$ERR" "$DEV" && has "$ERR" "none here"; then
    ok "an empty machine still names what a tree is holding"
else
    bad "a tree-only ledger went unmentioned on an empty machine: $ERR"
fi

echo
echo "=== $PASS passed, $FAIL failed ==="
[ "$FAIL" = 0 ]
