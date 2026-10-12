#!/usr/bin/env bash
# v15.1-C7: a touch is not sent at a point its target has already left.
#
# A consumer's player (2026-10-12) took its "go live" pill away by itself
# between smix's aim and the touch. The runner read what was under the
# point just before touching, saw the pill gone, and touched anyway: the
# overlay under it took the touch, and the tap was reported missed after
# the fact — or, where the layer under it does something, does that.
#
# The fixture's blink screen has a pill that shows for 700 ms and hides for
# 700 ms over a layer that takes a touch, each counting what it receives.
# Ten taps at the pill: on Android the layer under it receives none of them. Against a host that does not say what it aimed at (15.0)
# some of the ten arrive while the pill is away and the layer counts them.
#
# SMIX_C7_PLATFORMS=android runs the Android leg only.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/judged-run.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
# shellcheck source=../lib/e2e-devices.sh
ALIAS="${SMIX_C7_ANDROID:-$E2E_ANDROID}"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
TAPS=10

log()  { printf '[v15.1-c7] %s\n' "$*" >&2; }
fail() { printf '[v15.1-c7] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v15.1-c7] cannot judge: %s\n' "$*" >&2; exit 2; }

SERIAL="" WE_BOOTED=0 WE_UPPED=0 WORK=""
cleanup() {
  [ -n "$WORK" ] && rm -rf "$WORK"
  if [ "$WE_UPPED" = 1 ]; then
    "$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" \
      >/dev/null || printf '[v15.1-c7] warning: the runner was not stopped\n' >&2
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
}
trap cleanup EXIT

command -v adb >/dev/null 2>&1 || fail "no adb on PATH — this judges an Android runner and cannot"
[ -f "$APK" ] || fail "no fixture apk — run: bash scripts/dev/build-android-fixture.sh"
python3 "$ROOT/scripts/dev/fixture-apk-stamp.py" --check >&2 \
  || fail "the fixture apk on disk is not the one this tree builds"

if ! SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tail -1)" \
   || [ -z "$SERIAL" ] || ! adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1; then
  log "booting $ALIAS"
  with_deadline 300 "$SMIX" sim boot "$ALIAS" >/dev/null 2>&1 || fail "could not boot $ALIAS"
  WE_BOOTED=1
  SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tail -1)" || fail "$ALIAS was booted and does not resolve"
fi
case "$SERIAL" in
  emulator-*) : ;;
  *) fail "$ALIAS resolves to '$SERIAL', which is not an emulator — refusing" ;;
esac
for _ in $(seq 1 60); do
  adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1 && break
  sleep 2
done

adb -s "$SERIAL" install -r -g "$APK" >/dev/null 2>&1 || fail "could not install the fixture"
if ! curl -s -m 5 "http://localhost:$PORT/health" >/dev/null 2>&1; then
  "$SMIX" runner up "$ALIAS" --platform android --runner-port "$PORT" >/dev/null 2>&1 \
    || fail "the runner would not start on $SERIAL:$PORT"
  WE_UPPED=1
fi
WORK="$(mktemp -d)"

adb -s "$SERIAL" shell am start -W -S -n "$APPID/.BlinkActivity" >/dev/null 2>&1 \
  || fail "could not start the blink screen"

# ---- 1. at the runner: the same touch, without and with its target ----
#
# A flow's aim and its touch are a few tens of milliseconds apart, so a
# flow alone meets a pill on its way out only by chance (15.0 took ten taps
# without one). Here the touch is sent the moment the pill is seen gone,
# twice: once as a host that does not say what it aimed at (the touch goes
# in, and the layer under the pill counts it — the defect), once naming the
# pill (no touch: 409, and the count does not move).
probe_pill() { # prints "x y w h" of the pill when it is on screen, nothing when not
  curl -s -m 5 -H "App-Bundle-Id: $APPID" "http://localhost:$PORT/probe/tree?app=$APPID" | python3 -c '
import json, sys
try:
    roots = json.load(sys.stdin).get("roots") or []
except ValueError:
    raise SystemExit
stack = list(roots)
while stack:
    n = stack.pop()
    if n.get("testTag") == "blink-pill":
        l, t, r, b = n.get("visibleBounds") or n["bounds"]
        if r > l and b > t:
            print(l, t, r - l, b - t)
        raise SystemExit
    stack.extend(n.get("children") or [])'
}
counts() {
  "$SMIX" tree --json --port "$PORT" --device "$SERIAL" --platform android 2>/dev/null | python3 -c '
import json, re, sys
def walk(n):
    for k in ("text", "label"):
        m = re.search(r"pill=(\d+) under=(\d+)", n.get(k) or "")
        if m:
            print(m.group(1), m.group(2)); raise SystemExit
    for c in n.get("children") or []:
        walk(c)
walk(json.load(sys.stdin)["root"])
print("- -")'
}
display="$(curl -s -m 5 "http://localhost:$PORT/display")"
read -r dw dh <<<"$(printf '%s' "$display" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d["width"], d["height"])')"
box=""
for _ in $(seq 1 40); do box="$(probe_pill)"; [ -n "$box" ] && break; sleep 0.1; done
[ -n "$box" ] || fail "the pill never showed — this is not the blink screen"
read -r bx by bw bh <<<"$box"
nx=$(python3 -c "print(($bx + $bw / 2) / $dw)")
ny=$(python3 -c "print(($by + $bh / 2) / $dh)")
until_gone() { for _ in $(seq 1 60); do [ -z "$(probe_pill)" ] && return 0; sleep 0.05; done; return 1; }
tap_now() { # $1 extra JSON members; prints the HTTP status
  curl -s -m 10 -o "$WORK/tap.json" -w '%{http_code}' -H "App-Bundle-Id: $APPID" -X POST \
    "http://localhost:$PORT/tap-at-norm-coord" -d "{\"nx\":$nx,\"ny\":$ny,\"aimedBy\":\"semantics\"$1}"
}
read -r _ before <<<"$(counts)"
until_gone || fail "the pill never went away — this is not the blink screen"
status="$(tap_now '')"
sleep 0.5
read -r _ after_blind <<<"$(counts)"
log "a touch sent at the pill's place while it was away, naming nothing: HTTP $status, the layer under it $before → $after_blind"
[ "$after_blind" -gt "$before" ] || cannot_judge "the touch sent while the pill was away did not reach the layer under it — the screen this measures did not happen"
until_gone || fail "the pill never went away a second time"
status="$(tap_now ",\"aimedAt\":{\"identifier\":\"blink-pill\",\"label\":\"\",\"frame\":{\"x\":$bx,\"y\":$by,\"w\":$bw,\"h\":$bh}}")"
sleep 0.5
read -r _ after_named <<<"$(counts)"
log "the same touch naming the pill: HTTP $status ($(cat "$WORK/tap.json")), the layer under it $after_blind → $after_named"
[ "$status" = 409 ] || fail "a touch at a point the pill had left was sent (HTTP $status) though the pill was named"
[ "$after_named" = "$after_blind" ] || fail "the layer under the pill counted a touch the runner was told not to send"
grep -q '"target_gone_before_touch"' "$WORK/tap.json" || fail "the refusal does not say why: $(cat "$WORK/tap.json")"

# ---- 2. through a flow: ten taps at the pill --------------------------
adb -s "$SERIAL" shell am start -W -S -n "$APPID/.BlinkActivity" >/dev/null 2>&1 \
  || fail "could not restart the blink screen"
{ printf 'appId: %s\n---\n' "$APPID"
  for _ in $(seq 1 "$TAPS"); do printf -- '- tapOn: { id: blink-pill }\n'; done
} >"$WORK/taps.yaml"
rc=0
env -u SMIX_TAP_HIT_MISMATCH SMIX_RUNNER_PORT="$PORT" \
  "$SMIX_RUN" --device "$SERIAL" --platform android --no-launch "$WORK/taps.yaml" >"$WORK/taps.log" 2>&1 || rc=$?

counts="$(counts)"
read -r pill under <<<"$counts"
[ "$pill" != "-" ] || fail "the counts are not on the screen — this is not the blink screen"
log "after $TAPS taps at the pill: the pill counted $pill, the layer under it $under (flow exit $rc)"
[ "$under" = 0 ] || { tail -20 "$WORK/taps.log" >&2; fail "$under touch(es) aimed at the pill reached the layer under it"; }
[ "$rc" = 0 ] || { tail -20 "$WORK/taps.log" >&2; fail "the flow failed (exit $rc) though the layer under the pill received nothing"; }
# Not all ten: a pill that leaves while the finger is down cancels that
# press, and neither the pill nor the layer under it counts it (measured:
# 9 of 10). What is judged is where touches went, and that they went at all.
[ "$pill" -ge $((TAPS / 2)) ] || fail "the pill counted $pill of $TAPS taps — too few went in to judge where they went"
log "android: PASS"

case "${SMIX_C7_PLATFORMS:-both}" in
  android) log "V15.1-C7-E2E-PASS (Android only, as SMIX_C7_PLATFORMS asked)"; exit 0 ;;
  both) : ;;
  *) fail "SMIX_C7_PLATFORMS is '${SMIX_C7_PLATFORMS}' — it takes 'both' or 'android'" ;;
esac

# ---- iOS: the same two touches and the same ten taps -------------------
IOS_ALIAS="${SMIX_C7_IOS:-$E2E_IOS}"
gate_free_port IOS_PORT
IOS_APPID="jp.golia.smix.fixture"
FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
PROJECT="${SMIX_C7_RUNNER_PROJECT:-$ROOT/swift-bridge/SmixRunner.xcodeproj}"
command -v xcrun >/dev/null 2>&1 || cannot_judge "no xcrun"
[ -d "$FIXTURE" ] || fail "no iOS fixture at $FIXTURE (bash scripts/dev/build-fixture-app.sh)"
UDID="$("$SMIX" sim resolve "$IOS_ALIAS" 2>/dev/null | tr -d '[:space:]')" || true
[ -n "$UDID" ] || cannot_judge "no simulator resolves '$IOS_ALIAS'"
IOS_BOOTED=0 IOS_UPPED=0
ios_cleanup() {
  if [ "$IOS_UPPED" = 1 ]; then
    if ! said="$("$SMIX" runner down --device "$UDID" --runner-port "$IOS_PORT" 2>&1)"; then
      printf '[v15.1-c7] warning: the iOS runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
    fi
  fi
  if [ "$IOS_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true; fi
  cleanup
}
trap ios_cleanup EXIT
if [ "$(simulator_state "$UDID")" != Booted ]; then
  "$SMIX" sim boot "$UDID" >"$WORK/ios-boot.log" 2>&1 || fail "boot: $(tail -3 "$WORK/ios-boot.log")"
  IOS_BOOTED=1
fi
"$SMIX" sim install "$UDID" "$FIXTURE" >"$WORK/ios-install.log" 2>&1 || fail "install: $(tail -3 "$WORK/ios-install.log")"
SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX" runner up "$UDID" --bundle "$IOS_APPID" --runner-port "$IOS_PORT" \
  --runner-project "$PROJECT" >"$WORK/ios-up.log" 2>&1 || fail "runner up: $(tail -5 "$WORK/ios-up.log")"
IOS_UPPED=1

open_blink() {
  printf 'appId: %s\n---\n- launchApp:\n    arguments: ["-blink"]\n' "$IOS_APPID" >"$WORK/ios-open.yaml"
  SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX_RUN" --device "$UDID" "$WORK/ios-open.yaml" >"$WORK/ios-open.log" 2>&1 \
    || { tail -10 "$WORK/ios-open.log" >&2; fail "could not open the iOS blink screen"; }
}
ios_tree() {
  curl -s -m 10 -H "App-Bundle-Id: $IOS_APPID" "http://localhost:$IOS_PORT/tree"
}
ios_read() { # prints "pill_x pill_y pill_w pill_h app_w app_h pill under", "-" for a pill that is away
  ios_tree | python3 -c '
import json, re, sys
root = json.load(sys.stdin)
root = root.get("root", root)
b = root["bounds"]
pill, counts = None, None
stack = [root]
while stack:
    n = stack.pop()
    if n.get("identifier") == "blink-pill" and n.get("visible", True):
        pill = n["bounds"]
    m = re.search(r"pill=(\d+) under=(\d+)", (n.get("label") or "") + " " + (n.get("value") or ""))
    if n.get("identifier") == "blink-counts" and m:
        counts = m.groups()
    stack.extend(n.get("children") or [])
p = " ".join(str(pill[k]) for k in ("x", "y", "w", "h")) if pill else "- - - -"
c = " ".join(counts) if counts else "- -"
print(p, b["w"], b["h"], c)'
}
open_blink
seen=""
for _ in $(seq 1 40); do seen="$(ios_read)"; case "$seen" in -*) sleep 0.1 ;; *) break ;; esac; done
read -r px py pw ph aw ah _ before <<<"$seen"
[ "$px" != "-" ] || fail "the iOS pill never showed — this is not the blink screen"
inx=$(python3 -c "print(($px + $pw / 2) / $aw)")
iny=$(python3 -c "print(($py + $ph / 2) / $ah)")
ios_until_gone() { for _ in $(seq 1 60); do case "$(ios_read)" in -*) return 0 ;; esac; sleep 0.05; done; return 1; }
ios_tap() { # $1 extra JSON members; prints the HTTP status
  curl -s -m 15 -o "$WORK/ios-tap.json" -w '%{http_code}' -H "App-Bundle-Id: $IOS_APPID" -X POST \
    "http://localhost:$IOS_PORT/tap-at-norm-coord" -d "{\"nx\":$inx,\"ny\":$iny$1}"
}
ios_under() { ios_read | awk '{print $8}'; }
ios_until_gone || fail "the iOS pill never went away"
status="$(ios_tap '')"
sleep 0.5
blind="$(ios_under)"
window="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("latestDownOffsetMs","?"))' "$WORK/ios-tap.json")"
log "ios: a touch at the pill's place while it was away, naming nothing: HTTP $status, the layer under it $before → $blind; the runner read the point and touched within ${window} ms of the request"
[ "$blind" -gt "$before" ] || cannot_judge "on iOS the touch sent while the pill was away did not reach the layer under it — the screen this measures did not happen"
ios_until_gone || fail "the iOS pill never went away a second time"
status="$(ios_tap ",\"aimedAt\":{\"identifier\":\"blink-pill\",\"label\":\"GO LIVE\",\"frame\":{\"x\":$px,\"y\":$py,\"w\":$pw,\"h\":$ph}}")"
sleep 0.5
named="$(ios_under)"
log "ios: the same touch naming the pill: HTTP $status ($(cat "$WORK/ios-tap.json")), the layer under it $blind → $named"
[ "$status" = 409 ] || fail "on iOS a touch at a point the pill had left was sent (HTTP $status) though the pill was named"
[ "$named" = "$blind" ] || fail "on iOS the layer under the pill counted a touch the runner was told not to send"

open_blink
{ printf 'appId: %s\n---\n' "$IOS_APPID"
  for _ in $(seq 1 "$TAPS"); do printf -- '- tapOn: { id: blink-pill }\n'; done
} >"$WORK/ios-taps.yaml"
rc=0
env -u SMIX_TAP_HIT_MISMATCH SMIX_RUNNER_PORT="$IOS_PORT" \
  "$SMIX_RUN" --device "$UDID" "$WORK/ios-taps.yaml" >"$WORK/ios-taps.log" 2>&1 || rc=$?
read -r _ _ _ _ _ _ ipill iunder <<<"$(ios_read)"
# Reported, not judged: the iOS runner reads the point from a snapshot of
# the whole app, and a pill that leaves after that snapshot and before the
# touch is still touched — the window printed above. On Android the read is
# the probe's tree and the window is far shorter, so there the ten taps are
# judged.
log "ios: after $TAPS taps at the pill: the pill counted $ipill, the layer under it $iunder (flow exit $rc) — reported, not judged"
[ "$ipill" -ge $((TAPS / 2)) ] || fail "the iOS pill counted $ipill of $TAPS taps — too few went in to say anything"
log "V15.1-C7-E2E-PASS (android and ios)"
