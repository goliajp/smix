#!/usr/bin/env bash
# v10.2-C5: a scroll stops with its target wholly on screen, and the tap
# that follows lands on it — on both platforms, through one loop.
#
# The defect this is the instrument for, from a consumer's Android
# round (2026-09-22): the scroll
# stopped as soon as the target was in the tree AND overlapped the screen
# at all. A row crossing the bottom edge satisfies that with its middle
# below the edge, so `scrollUntilVisible` returned and the `tapOn` after
# it failed with `CentroidOutOfFrame { ny: 1.02 }`.
#
# So the target is chosen on the device, from the tree the flow's own
# eyes read: the row that crosses the bottom edge with its centre off
# screen. Naming a fixed row would test whichever geometry that screen
# happens to have.
#
# Both halves are checked, because a green here must mean the stop rule
# and not the flow happening to work:
#   - the flow passes: the scroll, then a tap on the row, then the label
#     the row's own onClick writes
#   - the row is wholly inside the app frame when the scroll stops, read
#     from the tree afterwards
#
# Run it against a binary built before the fix and the Android leg fails
# at the tap; that is how it was first seen to be red.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
AND_ALIAS="${SMIX_C5_ANDROID:-sim-smix-android-01}"
IOS_ALIAS="${SMIX_C5_IOS:-5D087114-ECB3-443C-8DDB-40EEF9CFB90C}"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
# One runner per platform, so two ports, both asked of the OS.
AND_PORT="$SMIX_RUNNER_PORT"
gate_free_port IOS_PORT
AND_APPID="dev.smix.fixture"
IOS_APPID="jp.golia.smix.fixture"
AND_APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
IOS_FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
IOS_PROJECT="$ROOT/swift-bridge/SmixRunner.xcodeproj"
WORK="$(mktemp -d)"
LEGS_RUN=0

log()  { printf '[c5-scroll] %s\n' "$*" >&2; }
step() { printf '[c5-scroll] --- %s\n' "$*" >&2; }
fail() { printf '[c5-scroll] FAIL: %s\n' "$*" >&2; exit 1; }
skip() { printf '[c5-scroll] SKIP: %s\n' "$*" >&2; exit 0; }

AND_SERIAL="" IOS_UDID=""
AND_WE_BOOTED=0 AND_WE_UPPED=0 IOS_WE_BOOTED=0 IOS_WE_UPPED=0
cleanup() {
  if [ "$AND_WE_UPPED" = 1 ]; then
    if ! said="$("$SMIX" runner down --platform android --device "$AND_SERIAL" --runner-port "$AND_PORT" 2>&1)"; then
      printf '[c5-scroll] warning: the Android runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
    fi
  fi
  if [ "$AND_WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$AND_SERIAL" >/dev/null 2>&1 || true; fi
  if [ "$IOS_WE_UPPED" = 1 ]; then
    if ! said="$("$SMIX" runner down --device "$IOS_UDID" --runner-port "$IOS_PORT" 2>&1)"; then
      printf '[c5-scroll] warning: the iOS runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
    fi
  fi
  if [ "$IOS_WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$IOS_UDID" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

[ -x "$SMIX" ] || fail "no smix binary at $SMIX (cargo build -p smix-cli)"

port_free() { ! curl -s "http://127.0.0.1:$1/health" 2>/dev/null | grep -q '"ok":true'; }

# The tree the FLOW reads, as json on stdout, with which reader answered.
#
# Through the CLI, which is the point: the flow and `smix tree` read the
# same screen through the same two eyes — the semantics probe when the
# app carries one, the accessibility reader otherwise — and the answer
# says which. Until I1 was closed the CLI took no bundle and so never
# asked the probe at all, and this had to curl `/probe/tree` to avoid
# measuring a different screen than the flow acts on.
flow_tree() { # $1 port  $2 device  $3 out.json
  "$SMIX" tree --json --port "$1" --device "$2" 2>/dev/null | grep -v '^kevy:' > "$3" || true
  head -c 1 "$3" | grep -q '{' || { echo "no-tree"; return 1; }
  python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["source"])' "$3"
}

# Every named node as (id, x, y, w, h), plus the screen.
#
# One shape, because there is one now: whichever reader answered, the
# tree arrives from `smix tree --json` as a root with `bounds` and
# `identifier` — a Compose testTag and a hosted View's resource id both
# land in the latter.
PY_NODES='
import json, sys
d = json.load(open(sys.argv[1]))
out = []
root = d["root"]
sw, sh = root["bounds"]["w"], root["bounds"]["h"]
def walk(n):
    ident = (n.get("identifier") or "").split("/")[-1]
    b = n["bounds"]
    if ident:
        out.append((ident, b["x"], b["y"], b["w"], b["h"]))
    for c in n.get("children") or []:
        walk(c)
walk(root)
'

# A row that crosses the bottom edge, in one of two shapes:
#
#   centre-out  its middle is below the edge as well. The old rule
#               stopped here AND the tap that followed missed, which is
#               what the consumer reported. Rows have to be tall enough
#               for that — the Compose fixture's are 110px.
#   any         it crosses at all. The old rule stopped here too, but
#               with the middle still inside the tap landed, so what
#               separates the rules is only where the scroll stopped.
#               The iOS fixture's rows are 52px and never give the
#               first shape.
#
# Each leg names the shape it means; neither falls back to the other,
# because they are different claims.
#
# Polled, not read once: straight after the navigation the probe answers
# with every row at (0, 0, 0, 0) for a frame or two, and a single look
# lands there and reports a screen that has no rows at all.
await_crossing_row() { # $1 port  $2 device  $3 out.json  $4 prefix  $5 shape
  local target=none
  for _ in $(seq 1 20); do
    flow_tree "$1" "$2" "$3" >/dev/null || { sleep 0.5; continue; }
    target="$(crossing_row "$3" "$4" "$5")"
    [ "$target" != none ] && { printf '%s\n' "$target"; return 0; }
    sleep 0.5
  done
  printf 'none\n'
  return 1
}

crossing_row() { # $1 tree.json  $2 id prefix  $3 shape
  python3 - "$1" "$2" "$3" <<PY
$PY_NODES
prefix, shape = sys.argv[2], sys.argv[3]
if shape not in ("centre-out", "any"):
    raise SystemExit(f"crossing_row: unknown shape {shape!r}")
for ident, x, y, w, h in out:
    if not ident.startswith(prefix):
        continue
    if y < sh < y + h and (shape == "any" or y + h / 2 > sh):
        print(ident)
        break
else:
    print("none")
PY
}

# Is the named element wholly inside the screen, as the tree reports it
# right now? The question the stop rule answers, asked afterwards and
# independently of the loop that decided it.
reach_of() { # $1 tree.json  $2 id
  python3 - "$1" "$2" <<PY
$PY_NODES
want = sys.argv[2]
# One node can be reported twice (iOS lists a cell and the control
# inside it under one identifier); identical boxes are one.
hits = sorted({(x, y, w, h) for ident, x, y, w, h in out if ident == want})
if not hits:
    print("absent")
elif len(hits) > 1:
    print(f"ambiguous({len(hits)})")
else:
    x, y, w, h = hits[0]
    print("full" if x >= -0.5 and y >= -0.5 and x + w <= sw + 0.5 and y + h <= sh + 0.5
          else f"partial(y={y:.0f}+{h:.0f} of {sh:.0f})")
PY
}

# ---- Android ---------------------------------------------------------
run_android() {
  command -v adb >/dev/null 2>&1 || { log "no adb — skipping the Android leg"; return 0; }
  AND_SERIAL="$("$SMIX" sim resolve "$AND_ALIAS" 2>/dev/null | grep -v '^kevy:' | tr -d '[:space:]')" || true
  [ -n "$AND_SERIAL" ] || { log "no emulator registered as '$AND_ALIAS' — skipping the Android leg"; return 0; }
  [ -f "$AND_APK" ] || { log "no fixture apk (bash scripts/dev/build-android-fixture.sh) — skipping the Android leg"; return 0; }
  port_free "$AND_PORT" || skip "port $AND_PORT already serves a runner — set SMIX_C5_ANDROID_PORT"

  step "Android: $AND_ALIAS ($AND_SERIAL)"
  if ! adb devices 2>/dev/null | grep -q "^${AND_SERIAL}[[:space:]]*device"; then
    "$SMIX" sim boot "$AND_SERIAL" >"$WORK/and-boot.log" 2>&1 || fail "emulator boot: $(tail -3 "$WORK/and-boot.log")"
    AND_WE_BOOTED=1
  fi
  "$SMIX" sim install "$AND_SERIAL" "$AND_APK" >"$WORK/and-install.log" 2>&1 \
    || fail "install: $(tail -3 "$WORK/and-install.log")"
  "$SMIX" runner up "$AND_SERIAL" --platform android --runner-port "$AND_PORT" >"$WORK/and-up.log" 2>&1 \
    || fail "runner up: $(tail -5 "$WORK/and-up.log")"
  AND_WE_UPPED=1

  # Bring the scrolling screen up, then ask the device which row is the
  # one crossing the edge. `launchApp` + a tap is the only way in: a flow
  # has no verb that starts an activity.
  cat >"$WORK/and-open.yaml" <<FLOW
appId: $AND_APPID
---
- launchApp:
    clearState: true
- tapOn:
    label: "open-scroll"
FLOW
  SMIX_RUNNER_PORT="$AND_PORT" "$SMIX" run --device "$AND_SERIAL" "$WORK/and-open.yaml" >"$WORK/and-open.log" 2>&1 \
    || { tail -10 "$WORK/and-open.log" >&2; fail "android: could not open the scrolling screen"; }
  local target
  target="$(await_crossing_row "$AND_PORT" "$AND_SERIAL" "$WORK/and-tree.json" scroll_row_ centre-out)" \
    || fail "android: no row crosses the bottom edge with its centre off screen — the screen this measures is not the screen it was written for"
  local eyes
  eyes="$(flow_tree "$AND_PORT" "$AND_SERIAL" "$WORK/and-tree.json")"
  local index="${target##*_}"
  log "android eyes=$eyes target=$target (its middle is below the bottom edge)"

  cat >"$WORK/and.yaml" <<FLOW
appId: $AND_APPID
---
- scrollUntilVisible:
    element:
      id: "$target"
    direction: DOWN
- tapOn:
    id: "$target"
- assertVisible: "tapped $index"
FLOW
  local out rc=0
  out="$(SMIX_RUNNER_PORT="$AND_PORT" "$SMIX" run --device "$AND_SERIAL" "$WORK/and.yaml" 2>&1 | grep -v '^kevy:')" || rc=$?
  if [ "$rc" -ne 0 ]; then
    printf '%s\n' "$out" | tail -20 >&2
    fail "android: the flow did not pass (the scroll stopped with $target still crossing the edge, and the tap went where its middle is)"
  fi
  flow_tree "$AND_PORT" "$AND_SERIAL" "$WORK/and-after.json" >/dev/null \
    || fail "android: no tree after the flow"
  local reach
  reach="$(reach_of "$WORK/and-after.json" "$target")"
  [ "$reach" = full ] || fail "android reach=$reach — the scroll stopped with the row not wholly on screen"
  log "android reach=full (flow passed: scroll → tap → the row's own label)"
  LEGS_RUN=$((LEGS_RUN + 1))
}

# ---- iOS -------------------------------------------------------------
run_ios() {
  command -v xcrun >/dev/null 2>&1 || { log "no xcrun — skipping the iOS leg"; return 0; }
  IOS_UDID="$("$SMIX" sim resolve "$IOS_ALIAS" 2>/dev/null | grep -v '^kevy:' | tr -d '[:space:]')" || true
  [ -n "$IOS_UDID" ] || { log "no simulator resolves '$IOS_ALIAS' — skipping the iOS leg"; return 0; }
  [ -d "$IOS_FIXTURE" ] || { log "no iOS fixture (bash scripts/dev/build-fixture-app.sh) — skipping the iOS leg"; return 0; }
  port_free "$IOS_PORT" || skip "port $IOS_PORT already serves a runner — set SMIX_C5_IOS_PORT"

  step "iOS: $IOS_UDID"
  if ! xcrun simctl list devices 2>/dev/null | grep -q "$IOS_UDID.*Booted"; then
    "$SMIX" sim boot "$IOS_UDID" >"$WORK/ios-boot.log" 2>&1 || fail "boot: $(tail -3 "$WORK/ios-boot.log")"
    IOS_WE_BOOTED=1
  fi
  "$SMIX" sim install "$IOS_UDID" "$IOS_FIXTURE" >"$WORK/ios-install.log" 2>&1 \
    || fail "install: $(tail -3 "$WORK/ios-install.log")"
  SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX" runner up "$IOS_UDID" --bundle "$IOS_APPID" --runner-port "$IOS_PORT" \
    --runner-project "$IOS_PROJECT" >"$WORK/ios-up.log" 2>&1 || fail "runner up: $(tail -5 "$WORK/ios-up.log")"
  IOS_WE_UPPED=1

  cat >"$WORK/ios-open.yaml" <<FLOW
appId: $IOS_APPID
---
- launchApp
FLOW
  SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX" run --device "$IOS_UDID" "$WORK/ios-open.yaml" >"$WORK/ios-open.log" 2>&1 \
    || { tail -10 "$WORK/ios-open.log" >&2; fail "ios: could not launch the fixture"; }
  local target
  target="$(await_crossing_row "$IOS_PORT" "$IOS_UDID" "$WORK/ios-tree.json" fixture-row- any)" \
    || fail "ios: no row crosses the bottom edge"
  local eyes
  eyes="$(flow_tree "$IOS_PORT" "$IOS_UDID" "$WORK/ios-tree.json")"
  log "ios eyes=$eyes target=$target (it crosses the bottom edge)"

  cat >"$WORK/ios.yaml" <<FLOW
appId: $IOS_APPID
---
- scrollUntilVisible:
    element:
      id: "$target"
    direction: DOWN
- tapOn:
    id: "$target"
FLOW
  local out rc=0
  out="$(SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX" run --device "$IOS_UDID" "$WORK/ios.yaml" 2>&1 | grep -v '^kevy:')" || rc=$?
  if [ "$rc" -ne 0 ]; then
    printf '%s\n' "$out" | tail -20 >&2
    fail "ios: the flow did not pass"
  fi
  flow_tree "$IOS_PORT" "$IOS_UDID" "$WORK/ios-after.json" >/dev/null || fail "ios: no tree after the flow"
  local reach
  reach="$(reach_of "$WORK/ios-after.json" "$target")"
  [ "$reach" = full ] || fail "ios reach=$reach — the scroll stopped with the row not wholly on screen"
  log "ios reach=full (flow passed: scroll → tap)"

  # The same loop, looking with OCR: the chain's tree layer names
  # nothing that exists, so only the `ocrText` can stop it. Before C5
  # this form ran an adapter-side loop of its own.
  cat >"$WORK/ios-ocr.yaml" <<FLOW
appId: $IOS_APPID
---
# Not clearState (backticks would run inside this heredoc, and on
# Xcode 27 its simctl spawn of /bin/rm fails with "Invalid or missing
# Program" and takes the launch with it). A relaunch is enough here:
# the fixture's list starts at the top either way.
- launchApp
- scrollUntilVisible:
    element:
      fallback:
        - id: "no-such-row-zz"
        - ocrText: "Row 30"
    direction: DOWN
FLOW
  rc=0
  out="$(SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX" run --device "$IOS_UDID" "$WORK/ios-ocr.yaml" 2>&1 | grep -v '^kevy:')" || rc=$?
  if [ "$rc" -ne 0 ]; then
    printf '%s\n' "$out" | tail -20 >&2
    fail "ios: the OCR chain did not stop the scroll"
  fi
  log "ios ocr=reached (a chain whose only usable layer is ocrText)"
  LEGS_RUN=$((LEGS_RUN + 1))
}

run_android
run_ios

# A run that skipped both legs proves nothing, and printing PASS for it
# is how a gate comes to mean "the machine had no devices".
[ "$LEGS_RUN" -ge 1 ] || fail "neither leg ran — no device answered, so nothing was measured"
log "C5-SCROLL-E2E-PASS ($LEGS_RUN leg(s))"
