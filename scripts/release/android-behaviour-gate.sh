#!/usr/bin/env bash
#
# Prove on a device that three Android fixes still work.
#
# All three shipped broken and nothing failed. A driver setter that
# accepted a value and dropped it, so --force-key-events was a switch
# wired to nothing. A second one just like it, so the runner never
# learned which app it was driving. And a README placeholder sitting in
# the view-id candidate list as though it were a real package, which
# meant every real app quietly took the slow path instead.
#
# What they had in common is not the bug, it is that no assertion
# anywhere would have gone red. This gate is three assertions that do.
#
# READ A2 AND A3 TOGETHER. A3 sends the App-Bundle-Id header by hand, so
# on its own it proves only that the runner, once told the package,
# builds a spelling that matches the app under test. That the driver
# actually sends that header is A2's job. Neither is a complete proof
# alone, and dropping either leaves the other looking sufficient.
#
# THE SECOND RUN IS PART OF THE PASS. The flow is written so that the
# --force-key-events flag alone decides the outcome: it types into a
# field id that is deliberately absent from the tree, which the flag
# makes irrelevant and its absence makes fatal. So the gate runs it
# twice and REQUIRES the second, flag-less run to fail. A smoke test
# that cannot fail when the fix is reverted is the thing this whole
# segment exists to get rid of.
#
# Env:
#   SMIX_ANDROID_SERIAL         — device; else the first emulator-*
#   SMIX_ANDROID_GATE_TIMEOUT_S — wall clock limit (default 600)
#   SMIX_BIN                    — smix binary (default: from PATH)

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TIMEOUT_S="${SMIX_ANDROID_GATE_TIMEOUT_S:-600}"
SMIX_BIN="${SMIX_BIN:-$(command -v smix)}"

APP="com.android.settings"
FLOW="$SCRIPT_DIR/android-behaviour/force-key-events.yaml"
PROXY_PORT=28090
RUNNER_PORT=28080

# From `uiautomator dump` on sim-smix-android-01 (android-33): the only
# clickable node whose short id mentions search. Re-derive it if the
# image changes — the command is in the failure message below.
PROBE_ID="search_action_bar"

WORK="${TMPDIR:-/tmp}/smix-android-behaviour"
WIRE="$WORK/wire.jsonl"
mkdir -p "$WORK"

PROXY_PID=""
cleanup() {
  [[ -n "$PROXY_PID" ]] && kill "$PROXY_PID" 2>/dev/null
  [[ -n "${SERIAL:-}" ]] && "$SMIX_BIN" runner down --platform android --device "$SERIAL" \
    >/dev/null 2>&1
  return 0
}
trap cleanup EXIT

die() {
  echo "android behaviour gate: $1" >&2
  exit 1
}

[[ -n "$SMIX_BIN" ]] || die "no smix binary on PATH (set SMIX_BIN)"
[[ -f "$FLOW" ]] || die "flow missing: $FLOW"

# --- device selection, before anything touches a device ------------------
#
# Same rule adb-guard enforces, restated here because the hook cannot
# see inside a script: it judges the text of the command it approves,
# and everything below is invisible to it once this file is invoked as
# `bash scripts/...`. Hiding the decision in here would be a bypass
# unless the decision comes with it. Emulator serials are allowlisted
# rather than known phones denylisted, so a newly attached device is
# safe by default.

SERIAL="${SMIX_ANDROID_SERIAL:-}"
if [[ -z "$SERIAL" ]]; then
  SERIAL="$(adb devices | awk '/^emulator-[0-9]+[[:space:]]+device$/ { print $1; exit }')"
fi
[[ -n "$SERIAL" ]] || die "no emulator attached. Start one:
  \"\$ANDROID_HOME/emulator/emulator\" -avd sim-smix-android-01 -port 5554 -no-snapshot-save &
  adb -s emulator-5554 wait-for-device"

if [[ ! "$SERIAL" =~ ^emulator-[0-9]+$ ]]; then
  die "refusing to drive '$SERIAL': not an emulator serial (emulator-NNNN).
  A physical device is often attached to a developer machine, and one has been
  wiped that way before. Pin an emulator via SMIX_ANDROID_SERIAL."
fi

echo "android behaviour gate: $APP on $SERIAL (timeout ${TIMEOUT_S}s)"

# --- bring up ------------------------------------------------------------
#
# `am start` rather than the flow's launchApp: foregroundCommand builds
# `-n <pkg>/.MainActivity`, and an AOSP app's launcher activity is not
# called that. Recorded as a product gap; not this gate's business.

adb -s "$SERIAL" shell am start -a android.settings.SETTINGS >/dev/null 2>&1 \
  || die "could not foreground $APP on $SERIAL"

"$SMIX_BIN" runner up "$SERIAL" --platform android > "$WORK/runner-up.log" 2>&1 \
  || die "runner up failed. Log: $WORK/runner-up.log"

: > "$WIRE"
python3 "$REPO_ROOT/scripts/dev/android-wire-record.py" \
  --listen "$PROXY_PORT" --forward "$RUNNER_PORT" --out "$WIRE" \
  > "$WORK/proxy.log" 2>&1 &
PROXY_PID=$!
# Detach from job control so the shell does not print "Terminated" over
# the gate's own verdict when cleanup kills it. Still killable by pid.
disown "$PROXY_PID" 2>/dev/null || true
sleep 2
kill -0 "$PROXY_PID" 2>/dev/null || die "wire recorder died. Log: $WORK/proxy.log"

# --- run, with a deadline ------------------------------------------------
#
# A release gate may fail; it may not hang. There is no timeout binary
# on macOS, so the deadline is a backgrounded child plus a poll.

run_flow() {
  local label="$1"; shift
  local logfile="$WORK/$label.log"
  ( "$SMIX_BIN" run --platform android --device "$SERIAL" --no-launch \
      --runner-port "$PROXY_PORT" "$@" "$FLOW" ) > "$logfile" 2>&1 &
  local pid=$!
  local waited=0
  while kill -0 "$pid" 2>/dev/null; do
    if (( waited >= TIMEOUT_S )); then
      kill "$pid" 2>/dev/null; sleep 2; kill -9 "$pid" 2>/dev/null
      die "flow '$label' did not finish within ${TIMEOUT_S}s. Log: $logfile"
    fi
    sleep 2
    waited=$(( waited + 2 ))
  done
  wait "$pid"
  return $?
}

# A1 — with the flag, the flow passes.
adb -s "$SERIAL" shell am start -a android.settings.SETTINGS >/dev/null 2>&1
sleep 3
if ! run_flow with-flag --force-key-events; then
  die "A1: the flow failed WITH --force-key-events. Log: $WORK/with-flag.log"
fi
echo "  A1a: flow passes with --force-key-events"

# A1 control — without it, the flow must fail. This is a pass condition.
adb -s "$SERIAL" shell am start -a android.settings.SETTINGS >/dev/null 2>&1
sleep 3
if run_flow without-flag; then
  die "A1: the flow ALSO passed without --force-key-events, so it no longer
  proves anything about that flag. Either the flow stopped depending on it, or
  resolution started finding an id that is supposed to be absent.
  Log: $WORK/without-flag.log"
fi
echo "  A1b: flow fails without it — the flag is what decides"

# A2 — every driving request carried the app under test.
#
# /health is exempt and only /health: it asks whether the runner is
# alive, not about any app. Every route that drives must carry it.
python3 - "$WIRE" "$APP" <<'PY' || die "A2: see above. Wire log: $WIRE"
import json, sys
rows = [json.loads(l) for l in open(sys.argv[1]) if l.strip()]
app = sys.argv[2]
driving = [r for r in rows if r["path"] != "/health"]
if not driving:
    print("A2: no driving requests recorded at all — the flow never reached the "
          "runner through the recorder, so this proves nothing", file=sys.stderr)
    sys.exit(1)
missing = [r for r in driving if (r.get("request_headers") or {}).get("App-Bundle-Id") != app]
if missing:
    paths = sorted({r["path"] for r in missing})
    print(f"A2: {len(missing)} of {len(driving)} driving requests did not carry "
          f"App-Bundle-Id: {app} — {paths}", file=sys.stderr)
    sys.exit(1)
print(f"  A2: {len(driving)}/{len(driving)} driving requests carried App-Bundle-Id: {app}")
PY

# A3 — the qualified spelling is what found the node.
adb -s "$SERIAL" shell am start -a android.settings.SETTINGS >/dev/null 2>&1
sleep 3
MATCH="$(curl -sS -D- -o /dev/null -X POST "http://localhost:$PROXY_PORT/tap-by-id" \
  -H "App-Bundle-Id: $APP" -d "{\"id\":\"$PROBE_ID\"}" \
  | awk -F': ' 'tolower($1) == "x-view-id-match" { print $2 }' | tr -d '\r')"

if [[ "$MATCH" != "qualified" ]]; then
  die "A3: X-View-Id-Match was '$MATCH', expected 'qualified'.
  'walk' means the qualified spelling missed and the manual walk answered —
  which is what the com.example.app placeholder caused, invisibly, because the
  walk returns the same node.
  If the image changed, re-derive the probe id:
    adb -s $SERIAL shell uiautomator dump /sdcard/d.xml
    adb -s $SERIAL shell cat /sdcard/d.xml | grep -o 'resource-id=\"$APP:id/[^\"]*\"'"
fi
echo "  A3: X-View-Id-Match: qualified (probe id: $PROBE_ID)"

echo "android behaviour gate: 3/3 assertions on $SERIAL ($APP)"
