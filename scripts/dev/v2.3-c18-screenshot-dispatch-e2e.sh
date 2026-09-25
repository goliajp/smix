#!/usr/bin/env bash
# v2.3-C18: `smix sim screenshot` dispatches by device kind.
#
# It used to call `simctl.screenshot` unconditionally, so a registered
# Android device had no screenshot on the CLI at all — even though
# `AndroidDeviceControl::screenshot` (`adb shell screencap -p`) has been
# implemented the whole time and the SDK/flow path always worked.
#
# Worth recording how this gap was mis-described first: the R5 research
# wrote it down as "/screenshot is not_implemented on the Android
# runner = an existing iOS/Android capability gap". `/screenshot` is not
# a route on *either* runner — iOS capture goes host-side through
# simctl/SmixCaptureHost — so the true statement was "neither has it, and
# both work anyway". Seeing `not_implemented` on one side and not
# checking the other turned "symmetric" into "Android is missing one".
#
# A screenshot is a sense capability, so which tool takes the picture is
# smix's problem, not the caller's. A phone was the exception when this
# was written — no path to its screen existed at all — and C20 removed
# the exception by giving the runner a `GET /screenshot`. What remains
# particular to a phone is that the picture comes from the runner, so
# there has to be one; step 1 pins how that is said.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
# shellcheck source=../lib/e2e-devices.sh
source "$ROOT/scripts/lib/e2e-devices.sh"
WORK="$(mktemp -d)"
OUT="$(mktemp)"

# Looked up in the real ledger, read-only, before this script switches to
# a ledger of its own. The phone is a device a person named, or a UDID
# with no device behind it; until 2026-09-25 it was the owner's iPhone,
# written here as a literal and registered as `phone` in the real ledger.
OUR_SERIAL="$("$SMIX" sim resolve "$E2E_ANDROID" 2>/dev/null | tail -1 || true)"
PHONE_UDID="$E2E_FAKE_IOS_UDID"
if [ -n "${SMIX_E2E_PHYSICAL_IOS:-}" ]; then
  e2e_physical_or_skip ios c18-shot
  PHONE_UDID="$("$SMIX" sim resolve "$E2E_PHYSICAL" 2>/dev/null | tail -1)"
fi
e2e_isolate_machine "$WORK"

log()  { printf '[c18-shot] %s\n' "$*"; }
step() { printf '[c18-shot] --- %s\n' "$*"; }
fail() { printf '[c18-shot] FAIL: %s\n' "$*" >&2; exit 1; }

cleanup() { rm -rf "$WORK" "$OUT"; }
trap cleanup EXIT
[ -x "$SMIX" ] || fail "no smix binary at $SMIX"
smix() { "$SMIX" "$@" 2>&1 || true; }

step "0. classification, which needs no devices"
( cd "$ROOT" && cargo test -p smix-cli --bin smix device_ref_is_classed ) > "$OUT" 2>&1 \
  || { tail -20 "$OUT"; fail "device_kind_of tests failed"; }
grep -qE "test result: ok\. [1-9]" "$OUT" || fail "the classification test did not run"
log "registry first, then shape"

cd "$WORK"
mkdir -p .smix

step "1. a physical iPhone with no runner: photographed if it offers capture, told what is missing if not"
# This step asserted a different sentence until C20. Back then there was
# no route to a phone's screen at all, and the honest answer was "this is
# a gap in smix". C20 built the route, so that sentence became false and
# the assertion had to move with it — a test still pinning it would have
# been demanding the tool lie.
#
# What is asserted now is the distinction that survived: a phone can be
# photographed, but only through a runner, so the failure has to say
# *which* thing is absent rather than "no screenshot".
smix sim register phone --udid "$PHONE_UDID" --kind physical-ios > "$OUT"
grep -q "registered:" "$OUT" || { cat "$OUT"; fail "registration failed"; }
# Since 10.2 there are two routes and the phone says which: Xcode 27's
# devicectl has `capture screenshot`, and a connected iPhone lists the
# capability. So the right outcome here depends on what the phone lists
# right now — asked of devicectl, not assumed.
PHONE_OFFERS_CAPTURE="$(xcrun devicectl list devices --json-output - 2>/dev/null | python3 -c '
import json, sys
for d in json.load(sys.stdin)["result"]["devices"]:
    if d.get("properties", {}).get("hardware", {}).get("udid") == sys.argv[1]:
        caps = [c.get("featureIdentifier") for c in d.get("capabilities", [])]
        print("yes" if "com.apple.coredevice.feature.capturescreenshot" in caps else "no"); break
else:
    print("no")' "$PHONE_UDID" 2>/dev/null || echo no)"
SMIX_RUNNER_PORT=22599 smix sim screenshot phone "$WORK/p.png" > "$OUT"
if [ "$PHONE_OFFERS_CAPTURE" = "yes" ]; then
  grep -q "screenshot:" "$OUT" || { cat "$OUT"; fail "a phone that offers capture was not photographed through devicectl"; }
  file "$WORK/p.png" | grep -q "PNG image data" || fail "the phone's screenshot is not a PNG"
  rm -f "$WORK/p.png"
  log "the phone offers capture: photographed through devicectl, no runner needed"
else
  grep -q "no runner is answering" "$OUT" || { cat "$OUT"; fail "did not name the missing runner"; }
  grep -q "smix runner up" "$OUT" || { cat "$OUT"; fail "no way forward named"; }
  # And it must still say why a phone needs one, or the instruction reads
  # as arbitrary to somebody used to simulators working without a runner.
  grep -q "no other way to be seen" "$OUT" \
    || { cat "$OUT"; fail "does not say why a phone differs from a simulator"; }
  [ -f "$WORK/p.png" ] && fail "a failed screenshot still wrote a file"
  log "the phone does not offer capture: named the missing runner, said why, wrote nothing"
fi

step "2. an Android device dispatches to adb, not simctl"
SERIAL="$OUR_SERIAL"
if [ -z "$SERIAL" ] || [ "$(adb -s "$SERIAL" get-state 2>/dev/null || true)" != device ]; then
  log "$E2E_ANDROID is not running — the half that proves a real capture cannot run"
  log "boot it (smix sim boot $E2E_ANDROID) and re-run"
  echo "C18-SCREENSHOT-DISPATCH-SKIP"
  exit 2
fi
log "emulator: $SERIAL"
smix sim register emu --udid "$SERIAL" --kind emulator > "$OUT"
grep -q "registered:" "$OUT" || { cat "$OUT"; fail "emulator registration failed"; }

smix sim screenshot emu "$WORK/a.png" > "$OUT"
grep -q "screenshot: $SERIAL" "$OUT" || { cat "$OUT"; fail "screenshot did not report success"; }

step "3. what came back is a real screen, not a plausible file"
# Size alone would pass for an error page written as bytes. The magic
# number says PNG and the IHDR says how big the screen is; a capture
# that silently produced nothing would fail both.
python3 - "$WORK/a.png" <<'PY'
import struct, sys
data = open(sys.argv[1], 'rb').read()
assert data[:8] == b'\x89PNG\r\n\x1a\n', f'not a PNG: {data[:8]!r}'
assert data[12:16] == b'IHDR', 'no IHDR chunk'
w, h = struct.unpack('>II', data[16:24])
assert w > 100 and h > 100, f'degenerate size {w}x{h}'
print(f'{w}x{h}, {len(data)} bytes')
PY
[ $? -eq 0 ] || fail "the captured file is not a usable screenshot"
log "captured $(python3 -c "
import struct,sys
d=open('$WORK/a.png','rb').read(); w,h=struct.unpack('>II',d[16:24]); print(f'{w}x{h} ({len(d)} bytes)')")"

echo "C18-SCREENSHOT-DISPATCH-PASS"
