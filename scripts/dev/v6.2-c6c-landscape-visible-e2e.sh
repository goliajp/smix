#!/usr/bin/env bash
# v6.2-C6c: in landscape, `tree` reports on-screen elements as visible.
#
# The human/JSON tree judged visibility against an appFrame cached at
# portrait startup (402 wide). Once the app locked to landscape (root 874
# wide), every node past x=402 was called off-screen — landscape-counter
# (x=407) reported visible=false while it was plainly on screen, and
# landscape-increment / landscape-exit (inside 402) reported true. The fix
# judges against the snapshot's own root frame. This drives the real
# landscape stage and requires all three visible=true.
#
# iOS only. Boots the registered smix-ios sim, and tears down only what it
# started (including the xcodebuild pinned to its own udid).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
ALIAS="${SMIX_C6C_IOS:-smix-ios}"
PORT="${SMIX_C6C_PORT:-22090}"
BUNDLE="jp.golia.smix.fixture"
FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
WORK="$(mktemp -d)"

log()  { printf '[c6c] %s\n' "$*" >&2; }
step() { printf '[c6c] --- %s\n' "$*" >&2; }
fail() { printf '[c6c] FAIL: %s\n' "$*" >&2; exit 1; }
skip() { printf '[c6c] SKIP: %s\n' "$*" >&2; exit 0; }

UDID="" WE_BOOTED=0 WE_UPPED=0
cleanup() {
  [ "$WE_UPPED" = 1 ]  && "$SMIX" down --device "$UDID" >/dev/null 2>&1 || true
  if [ -n "$UDID" ]; then
    P="$(pgrep -f "xcodebuild.*$UDID" 2>/dev/null || true)"
    [ -n "$P" ] && { kill -INT $P 2>/dev/null || true; sleep 1; kill -9 $P 2>/dev/null || true; }
  fi
  [ "$WE_BOOTED" = 1 ] && "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true
  rm -rf "$WORK"
}
trap cleanup EXIT

[ -x "$SMIX" ] || fail "no smix binary at $SMIX"
command -v xcrun >/dev/null 2>&1 || skip "no xcrun — this needs Xcode"
UDID="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | grep -v '^kevy:' | tr -d '[:space:]')" || true
[ -n "$UDID" ] || skip "no sim registered as '$ALIAS'"
[ -d "$FIXTURE" ] || skip "no iOS fixture at $FIXTURE"

step "boot $ALIAS ($UDID), install fixture, runner up --bundle $BUNDLE"
if ! xcrun simctl list devices 2>/dev/null | grep -q "$UDID.*Booted"; then
  "$SMIX" sim boot "$UDID" >"$WORK/boot.log" 2>&1 || fail "sim boot failed: $(tail -3 "$WORK/boot.log")"
  WE_BOOTED=1
fi
"$SMIX" sim install "$UDID" "$FIXTURE" >"$WORK/install.log" 2>&1 || fail "install failed: $(tail -3 "$WORK/install.log")"
# Build from the repo's own runner project so this gate tests the checked-out
# swift-bridge (with the C6c fix), not the install-shipped copy that only
# re-syncs on version drift.
SMIX_RUNNER_PORT="$PORT" "$SMIX" runner up "$UDID" --bundle "$BUNDLE" --runner-port "$PORT" \
  --runner-project "$ROOT/swift-bridge/SmixRunner.xcodeproj" >"$WORK/up.log" 2>&1 \
  || fail "runner up failed: $(tail -6 "$WORK/up.log")"
WE_UPPED=1

step "launch, enter landscape stage"
cat >"$WORK/flow.yaml" <<FLOW
appId: $BUNDLE
---
- launchApp
- tapOn:
    id: landscape-enter
FLOW
SMIX_RUNNER_PORT="$PORT" "$SMIX" run --device "$UDID" "$WORK/flow.yaml" >"$WORK/run.log" 2>&1 \
  || fail "could not enter landscape stage: $(grep -v '^kevy:' "$WORK/run.log" | tail -4)"

# Wait for the landscape stage to lay out.
for _ in $(seq 1 15); do
  SMIX_RUNNER_PORT="$PORT" "$SMIX" tree --json --device "$UDID" 2>/dev/null | grep -v '^kevy:' | grep -q 'landscape-counter' && break
  sleep 1
done

step "assert landscape-counter / -increment / -exit are all visible=true"
TREE="$(SMIX_RUNNER_PORT="$PORT" "$SMIX" tree --json --device "$UDID" 2>/dev/null | grep -v '^kevy:')"
printf '%s' "$TREE" | python3 -c "
import sys, json
tree = json.load(sys.stdin)
want = {'landscape-counter', 'landscape-increment', 'landscape-exit'}
seen = {}
def walk(n):
    i = n.get('identifier')
    if i in want:
        seen[i] = n.get('visible')
    for c in n.get('children', []) or []:
        walk(c)
walk(tree)
missing = want - set(seen)
if missing:
    print('MISSING:' + ','.join(sorted(missing))); sys.exit(2)
bad = {k: v for k, v in seen.items() if v is not True}
if bad:
    print('NOT_VISIBLE:' + json.dumps(bad)); sys.exit(3)
print('OK:' + json.dumps(seen))
" >"$WORK/verdict.txt" 2>&1 || {
  V="$(cat "$WORK/verdict.txt")"
  case "$V" in
    MISSING:*) fail "landscape nodes not in tree (${V#MISSING:}) — the stage did not come up" ;;
    NOT_VISIBLE:*) fail "a landscape node reports visible!=true: ${V#NOT_VISIBLE:} — the C6c bug (coordinate-space mismatch) is present" ;;
    *) fail "verdict check errored: $V" ;;
  esac
}
log "$(cat "$WORK/verdict.txt")"
log "v6.2-C6c PASS: landscape counter / increment / exit all visible=true"
