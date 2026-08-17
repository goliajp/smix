#!/usr/bin/env bash
# v6.2-C6d: attach really times out, really retries, really attaches.
#
# The [5.0.0] CHANGELOG said this had never been watched on a device —
# only a decision table and a TCP stub. This drives it on an iOS
# simulator: an #[ignore] integration test injects a first-attempt
# timeout through the up_on_with seam, then lets the real bring-up
# foreground the app (xcrun simctl launch) and attach. The proof is not a
# return code — after it, `smix tree` must show the fixture, i.e. the
# runner the attach brought up can actually drive the app.
#
# iOS only (decide_after_timeout refuses Physical). Tears down only what
# it started, including the xcodebuild pinned to its own udid.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
ALIAS="${SMIX_C6D_IOS:-smix-ios}"
PORT="${SMIX_C6D_PORT:-22092}"
BUNDLE="jp.golia.smix.fixture"
FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
PROJECT="$ROOT/swift-bridge/SmixRunner.xcodeproj"
WORK="$(mktemp -d)"

log()  { printf '[c6d] %s\n' "$*" >&2; }
step() { printf '[c6d] --- %s\n' "$*" >&2; }
fail() { printf '[c6d] FAIL: %s\n' "$*" >&2; exit 1; }
skip() { printf '[c6d] SKIP: %s\n' "$*" >&2; exit 0; }

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
[ -d "$PROJECT" ] || skip "no runner project at $PROJECT"
# Refuse to collide with a runner already on this port — up_on_with would
# (correctly) refuse to kill an unrecorded runner, so pick a free port
# instead of fighting one somebody else owns.
if curl -s "http://127.0.0.1:$PORT/health" 2>/dev/null | grep -q '"ok":true'; then
  skip "port $PORT already serves a runner — set SMIX_C6D_PORT to a free port"
fi

step "boot $ALIAS ($UDID), install fixture"
if ! xcrun simctl list devices 2>/dev/null | grep -q "$UDID.*Booted"; then
  "$SMIX" sim boot "$UDID" >"$WORK/boot.log" 2>&1 || fail "sim boot failed: $(tail -3 "$WORK/boot.log")"
  WE_BOOTED=1
fi
"$SMIX" sim install "$UDID" "$FIXTURE" >"$WORK/install.log" 2>&1 || fail "install failed: $(tail -3 "$WORK/install.log")"

step "inject first-attempt timeout, let the real bring-up attach"
# The #[ignore] test drives up_on_with: first attempt injected TimedOut,
# then real xcodebuild attach. It brings a runner up on $PORT that
# outlives this process (WE_UPPED so cleanup stops it).
WE_UPPED=1
SMIX_C6D_UDID="$UDID" SMIX_C6D_PORT="$PORT" SMIX_C6D_BUNDLE="$BUNDLE" \
  SMIX_C6D_RUNNER_PROJECT="$PROJECT" \
  cargo test -p smix-capsule --test attach_on_device -- --ignored --nocapture \
  >"$WORK/test.log" 2>&1 \
  || { grep -v '^kevy:' "$WORK/test.log" | tail -20 >&2; fail "attach-on-device test failed (see above) — the retry did not bring the runner up, or attach_flags != [false, true]"; }
grep -q 'first_timeout_then_real_attach_brings_the_runner_up ... ok' "$WORK/test.log" \
  || { grep -v '^kevy:' "$WORK/test.log" | tail -20 >&2; fail "the attach test did not report ok"; }
log "injected timeout → real simctl launch → real attach: Ok, attach_flags=[false, true]"

step "the attach-brought runner must actually drive the app (tree shows the fixture)"
# The attach foregrounds a fresh launch; give the home screen a moment to
# lay out before reading, then judge the file rather than a pipe grep -q
# (which would SIGPIPE the producer).
FOUND=0
for _ in $(seq 1 15); do
  SMIX_RUNNER_PORT="$PORT" "$SMIX" tree --json --device "$UDID" 2>/dev/null \
    | grep -v '^kevy:' >"$WORK/tree.json" || true
  if grep -q 'landscape-enter' "$WORK/tree.json"; then FOUND=1; break; fi
  sleep 1
done
[ "$FOUND" = 1 ] \
  || fail "the runner the attach brought up cannot see the fixture — tree has no 'landscape-enter'"
log "tree via the attached runner shows the fixture (landscape-enter present)"

log "v6.2-C6d PASS: really timed out, really retried, really attached, really drives the app"
