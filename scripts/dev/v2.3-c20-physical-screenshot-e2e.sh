#!/usr/bin/env bash
# v2.3-C20: a physical iPhone can be photographed.
#
# Until 2026-08-06 it could not, by anyone. `simctl io screenshot` covers
# simulators; Apple exposes no equivalent for a phone through `devicectl`;
# and the runner — where `XCUIScreen.main.screenshot()` has been running
# since the OCR work — had no route that handed the pixels back. So a real
# device could be tapped, typed into and read, but never seen. C18 made
# that an honest refusal; C20 removes the need for one.
#
# The capture is proven against the device's own geometry rather than by
# file size: a PNG from anywhere passes a magic-number check, and a
# zero-byte file passes nothing except a test that only looks for
# "success". The screen's point size comes from `/tree` and the image's
# pixel size from its IHDR; a real capture is an integer scale apart.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
ALIAS="${SMIX_PHYSICAL_ALIAS:-phone}"
BUNDLE="${SMIX_PHYSICAL_BUNDLE:-com.apple.Preferences}"
PORT="${SMIX_RUNNER_PORT:-22087}"
SHOT="$(mktemp -d)/shot.png"
OUT="$(mktemp)"

log()  { printf '[c20-shot] %s\n' "$*"; }
step() { printf '[c20-shot] --- %s\n' "$*"; }
fail() { printf '[c20-shot] FAIL: %s\n' "$*" >&2; exit 1; }

cleanup() {
  # The image is of somebody's phone. It proved what it was for; it does
  # not need to outlive the run.
  rm -rf "$(dirname "$SHOT")" "$OUT"
}
trap cleanup EXIT
cd "$ROOT"
[ -x "$SMIX" ] || fail "no smix binary at $SMIX"

step "0. the envelope, proven without a device"
( cd swift-bridge && swift test --filter ScreenshotRoute ) > "$OUT" 2>&1 \
  || { tail -20 "$OUT"; fail "ScreenshotRoute tests failed"; }
grep -qE "Executed 3 tests, with 0 failures" "$OUT" || fail "the route tests did not all run"
log "route envelope: 3 tests (bytes verbatim, no-pixels refuses, empty refuses)"

( cd "$ROOT" && cargo test -p smix-capsule --lib screenshot_failure ) > "$OUT" 2>&1 \
  || { tail -20 "$OUT"; fail "the failure-message tests failed"; }
log "each failure names a different fix"

step "1. is there a registered, reachable phone with a runner up?"
UDID="$(cargo run -q -p smix-usbmux --example first_device 2>/dev/null || true)"
if [ -z "$UDID" ]; then
  log "no iOS device on usbmux — nothing to photograph"
  echo "C20-PHYSICAL-SCREENSHOT-SKIP"
  exit 0
fi
if ! "$SMIX" sim resolve "$ALIAS" >/dev/null 2>&1; then
  log "device $UDID is attached but no alias '$ALIAS' is registered"
  log "register it: smix sim register $ALIAS --udid $UDID --kind physical-ios"
  echo "C20-PHYSICAL-SCREENSHOT-SKIP"
  exit 0
fi
if ! curl -s -m 5 "http://127.0.0.1:$PORT/health" >/dev/null 2>&1; then
  log "no runner answering on $PORT — a phone has no other way to be seen"
  log "smix runner up $ALIAS --bundle $BUNDLE   (then re-run)"
  echo "C20-PHYSICAL-SCREENSHOT-SKIP"
  exit 0
fi
log "phone $UDID, runner on $PORT"

step "2. the screen comes back as a PNG"
"$SMIX" sim screenshot "$ALIAS" "$SHOT" > "$OUT" 2>&1 \
  || { grep -v '^kevy:' "$OUT"; fail "screenshot failed"; }
[ -s "$SHOT" ] || fail "the file is empty — a zero-byte PNG is the failure this exists to avoid"

step "3. and it is THIS device's screen, not a picture from anywhere"
curl -s -m 15 "http://127.0.0.1:$PORT/tree" > "$OUT" 2>&1 || fail "/tree did not answer"
python3 - "$SHOT" "$OUT" <<'PY'
import json, struct, sys
png, tree_path = sys.argv[1], sys.argv[2]
data = open(png, 'rb').read()
assert data[:8] == b'\x89PNG\r\n\x1a\n', f'not a PNG: {data[:8]!r}'
assert data[12:16] == b'IHDR', 'no IHDR chunk'
pw, ph = struct.unpack('>II', data[16:24])

bounds = json.load(open(tree_path)).get('bounds', {})
tw, th = bounds.get('w', 0), bounds.get('h', 0)
assert tw > 0 and th > 0, f'/tree gave no usable bounds: {bounds}'

# Points to pixels is an integer scale on every iPhone Apple ships. A
# capture of some other screen would land off it.
sw, sh = pw / tw, ph / th
assert abs(sw - round(sw)) < 0.02 and abs(sh - round(sh)) < 0.02, \
    f'{pw}x{ph} is not an integer scale of the {tw}x{th} screen (got {sw:.3f}x{sh:.3f})'
assert round(sw) == round(sh), f'anisotropic scale {sw:.3f} vs {sh:.3f}'
assert round(sw) in (1, 2, 3), f'implausible scale {round(sw)}'
print(f'{pw}x{ph} px = {tw}x{th} pt at {round(sw)}x, {len(data)} bytes')
PY
[ $? -eq 0 ] || fail "the capture does not match this device's screen"
log "captured and matched to the device's own geometry"

echo "C20-PHYSICAL-SCREENSHOT-PASS"
