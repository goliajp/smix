#!/usr/bin/env bash
# v18.0-C2: a run says where its time went.
#
# A consumer wanted each flow's time and the fixed costs inside it, and
# smix gave a JUnit time of 0 for every flow, a JSON summary whose steps
# were empty, and per-step times only in files nobody had told them about.
#
# Judged on each platform against the fixture's zoom screen, from what the
# run writes rather than from what it says about itself:
#   1. `--format json` carries `timing`: setup + steps + teardown is not
#      more than the total; every step has a time; a tap has find, settle
#      and touch in that order, adding up to no more than the step; the
#      repeat holds its two taps, unnumbered; on iOS launchApp has launch
#      and appear
#   2. `run-summary.json` under `--debug-output` has the same steps
#   3. stderr ends every step with its time and the run with a `time:` line
#   4. `--format junit` over two flows is one XML document a parser reads,
#      two suites, each flow with a time above zero
#
# SMIX_C2_PLATFORMS=ios or =android runs one leg.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/judged-run.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port PORT
PLATFORMS="${SMIX_C2_PLATFORMS:-ios android}"
IOS_ALIAS="${SMIX_C2_IOS:-$E2E_IOS}"
ANDROID_ALIAS="${SMIX_C2_ANDROID:-$E2E_ANDROID}"
IOS_APP="jp.golia.smix.fixture"
ANDROID_APP="dev.smix.fixture"
FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
PROJECT="$ROOT/swift-bridge/SmixRunner.xcodeproj"
WORK="$(mktemp -d)"

log()  { printf '[v18.0-c2] %s\n' "$*" >&2; }
fail() { printf '[v18.0-c2] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v18.0-c2] cannot judge: %s\n' "$*" >&2; exit 2; }

DEVICE="" PLATFORM="" APP="" UP=0 IOS_DEVICE="" ANDROID_DEVICE="" WE_BOOTED_IOS=0 WE_BOOTED_ANDROID=0
runner_down() {
  [ "$UP" = 1 ] || return 0
  local said
  if ! said="$("$SMIX" runner down --platform "$PLATFORM" --device "$DEVICE" --runner-port "$PORT" 2>&1)"; then
    log "warning: the $PLATFORM runner on $PORT was not stopped:"
    printf '%s\n' "$said" | tail -3 >&2
  fi
  UP=0
}
cleanup() {
  runner_down
  if [ "$WE_BOOTED_IOS" = 1 ]; then "$SMIX" sim shutdown "$IOS_DEVICE" >/dev/null 2>&1 || true; fi
  if [ "$WE_BOOTED_ANDROID" = 1 ]; then "$SMIX" sim shutdown "$ANDROID_DEVICE" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

write_flows() {
  if [ "$PLATFORM" = ios ]; then
    printf 'appId: %s\n---\n- launchApp: { clearState: true, arguments: ["-zoom"] }\n' "$APP" >"$WORK/a.yaml"
  else
    printf 'appId: %s\n---\n' "$APP" >"$WORK/a.yaml"
  fi
  cat >>"$WORK/a.yaml" <<'FLOW'
- extendedWaitUntil: { visible: { id: zoom-reset }, timeout: 10000 }
- tapOn: { id: zoom-reset }
- repeat:
    times: 2
    commands:
      - tapOn: { id: zoom-reset }
FLOW
  printf 'appId: %s\n---\n- tapOn: { id: zoom-reset }\n' "$APP" >"$WORK/b.yaml"
}

open_zoom() {
  [ "$PLATFORM" = android ] || return 0
  adb -s "$DEVICE" shell am start -W -S -n "$APP/.ZoomActivity" >/dev/null 2>&1 \
    || fail "android: could not start the zoom screen"
}

judge() {
  write_flows
  open_zoom
  log "$PLATFORM: 1-3. one flow, --format json and --debug-output"
  "$SMIX_RUN" --device "$DEVICE" --runner-port "$PORT" --format json --debug-output "$WORK/debug" \
    "$WORK/a.yaml" >"$WORK/out.json" 2>"$WORK/err.log" \
    || { tail -15 "$WORK/err.log" >&2; fail "$PLATFORM: the flow did not pass"; }
  python3 - "$WORK/out.json" "$WORK/debug/run-summary.json" "$PLATFORM" <<'PY' || fail "$PLATFORM: the timing does not hold (above)"
import json, sys
out = json.loads(open(sys.argv[1]).read().strip().splitlines()[-1])
summary = json.load(open(sys.argv[2]))
platform = sys.argv[3]
def no(msg):
    print(f"[v18.0-c2]   {msg}", file=sys.stderr); sys.exit(1)
t = out.get("timing") or no("--format json has no timing")
steps = t["steps"]
want = 4 if platform == "ios" else 3
if len(steps) != want: no(f"{len(steps)} top-level steps, the flow has {want}")
parts = t["setupMs"] + t["stepsMs"] + t["teardownMs"]
if parts > t["totalMs"]: no(f"setup+steps+teardown {parts} ms is more than the total {t['totalMs']} ms")
if t["setupMs"] <= 0: no("setupMs is 0: the time before the first step was not measured")
if sum(s["wallMs"] for s in steps) != t["stepsMs"]: no("stepsMs is not the steps added up")
for s in steps:
    if s["wallMs"] <= 0: no(f"step {s['index']} took 0 ms")
tap = steps[-2]
names = [p["name"] for p in tap["phases"]]
if names[-3:] != ["find", "settle", "touch"]: no(f"the tap's phases are {names}")
if sum(p["ms"] for p in tap["phases"]) > tap["wallMs"] + 2: no(f"the tap's phases add up past its {tap['wallMs']} ms")
rep = steps[-1]
if len(rep["steps"]) != 2: no(f"the repeat holds {len(rep['steps'])} steps, not its two taps")
if any(c["index"] is not None for c in rep["steps"]): no("a step inside the repeat is numbered")
if any([p["name"] for p in c["phases"]][-1:] != ["touch"] for c in rep["steps"]): no("a tap inside the repeat has no touch phase")
if platform == "ios":
    launch = [p["name"] for p in steps[0]["phases"]]
    if launch != ["launch", "appear"]: no(f"launchApp's phases are {launch}")
st = summary.get("timing") or no("run-summary.json has no timing")
if [s["summary"] for s in st["steps"]] != [s["summary"] for s in steps]: no("run-summary.json lists other steps than --format json")
if len(out.get("steps") or []) != want: no("--format json with --debug-output lists no step records")
print(f"[v18.0-c2]   total {t['totalMs']} ms = setup {t['setupMs']} + steps {t['stepsMs']} + teardown {t['teardownMs']} + between; tap {names[-3:]} {[p['ms'] for p in tap['phases'][-3:]]} of {tap['wallMs']} ms", file=sys.stderr)
PY
  local ended
  ended="$(grep -cE '^STEP [0-9]+: .* → ok \([0-9]+ ms\)$' "$WORK/err.log" || true)"
  [ "$ended" -ge 3 ] || fail "$PLATFORM: stderr ended $ended steps with a time"
  grep -qE '^time: [0-9]+ ms — setup [0-9]+ ms, steps [0-9]+ ms, teardown [0-9]+ ms$' "$WORK/err.log" \
    || fail "$PLATFORM: no time line on stderr"

  log "$PLATFORM: 4. two flows, --format junit"
  open_zoom
  "$SMIX_RUN" --device "$DEVICE" --runner-port "$PORT" --format junit \
    "$WORK/a.yaml" "$WORK/b.yaml" >"$WORK/junit.xml" 2>"$WORK/junit.log" \
    || { tail -15 "$WORK/junit.log" >&2; fail "$PLATFORM: the two flows did not pass"; }
  python3 - "$WORK/junit.xml" <<'PY' || fail "$PLATFORM: the junit document does not hold (above)"
import sys, xml.etree.ElementTree as ET
def no(msg):
    print(f"[v18.0-c2]   {msg}", file=sys.stderr); sys.exit(1)
try:
    root = ET.fromstring(open(sys.argv[1]).read().strip())
except ET.ParseError as e:
    no(f"not one XML document: {e}")
if root.tag != "testsuites": no(f"the root is <{root.tag}>, not <testsuites>")
cases = root.findall("./testsuite/testcase")
if [c.get("name") for c in cases] != ["a", "b"]: no(f"testcases {[c.get('name') for c in cases]}")
for c in cases:
    if float(c.get("time", "0")) <= 0: no(f"{c.get('name')} has time {c.get('time')}")
print(f"[v18.0-c2]   one document, times {[c.get('time') for c in cases]} s", file=sys.stderr)
PY
  echo "WHERE-THE-TIME-WENT-PASS $PLATFORM"
}

ios_leg() {
  command -v xcrun >/dev/null 2>&1 || cannot_judge "no xcrun"
  DEVICE="$("$SMIX" sim resolve "$IOS_ALIAS" 2>/dev/null | tail -1 | tr -d '[:space:]')" || true
  [ -n "$DEVICE" ] || cannot_judge "no simulator resolves '$IOS_ALIAS'"
  [ -d "$FIXTURE" ] || fail "no iOS fixture (bash scripts/dev/build-fixture-app.sh)"
  PLATFORM=ios APP="$IOS_APP"
  if [ "$(simulator_state "$DEVICE")" != Booted ]; then
    with_deadline 300 "$SMIX" sim boot "$DEVICE" >"$WORK/boot.log" 2>&1 || fail "boot: $(tail -3 "$WORK/boot.log")"
    IOS_DEVICE="$DEVICE"
    WE_BOOTED_IOS=1
  fi
  "$SMIX" sim install "$DEVICE" "$FIXTURE" >"$WORK/install.log" 2>&1 || fail "install: $(tail -3 "$WORK/install.log")"
  "$SMIX" runner up "$DEVICE" --bundle "$IOS_APP" --runner-port "$PORT" --runner-project "$PROJECT" \
    >"$WORK/up.log" 2>&1 || fail "ios runner up: $(tail -5 "$WORK/up.log")"
  UP=1
  judge
  runner_down
}

android_leg() {
  command -v adb >/dev/null 2>&1 || cannot_judge "no adb"
  [ -f "$APK" ] || fail "no fixture apk (bash scripts/dev/build-android-fixture.sh)"
  python3 "$ROOT/scripts/dev/fixture-apk-stamp.py" --check >&2 \
    || fail "the fixture apk on disk is not the one this tree builds"
  PLATFORM=android APP="$ANDROID_APP"
  if ! DEVICE="$("$SMIX" sim resolve "$ANDROID_ALIAS" 2>/dev/null | tail -1)" || [ -z "$DEVICE" ]; then
    with_deadline 300 "$SMIX" sim boot "$ANDROID_ALIAS" >"$WORK/boot.log" 2>&1 || fail "boot: $(tail -3 "$WORK/boot.log")"
    DEVICE="$("$SMIX" sim resolve "$ANDROID_ALIAS" 2>/dev/null | tail -1)" || fail "$ANDROID_ALIAS was booted and does not resolve"
    ANDROID_DEVICE="$DEVICE"
    WE_BOOTED_ANDROID=1
  fi
  case "$DEVICE" in
    emulator-*) : ;;
    *) fail "$ANDROID_ALIAS resolves to '$DEVICE', which is not an emulator — refusing" ;;
  esac
  for _ in $(seq 1 60); do
    adb -s "$DEVICE" shell getprop sys.boot_completed 2>/dev/null | grep -q 1 && break
    sleep 2
  done
  adb -s "$DEVICE" install -r -g "$APK" >/dev/null 2>&1 || fail "could not install the fixture"
  "$SMIX" runner up "$DEVICE" --platform android --runner-port "$PORT" >"$WORK/up.log" 2>&1 \
    || fail "android runner up: $(tail -5 "$WORK/up.log")"
  UP=1
  judge
  runner_down
}

for p in $PLATFORMS; do
  case "$p" in
    ios) ios_leg ;;
    android) android_leg ;;
    *) fail "unknown platform '$p'" ;;
  esac
done
