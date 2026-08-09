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
# The second subject. Settings is a system app — preinstalled, stable
# ids, windows owned by the system — so A1–A3 prove things about the
# platform's own app and nothing about anybody else's. A4 drives an
# ordinary one, which is the shape a consumer actually has. Added
# alongside rather than instead: the Settings assertions cover the
# system window layer, which the fixture cannot.
FIXTURE_APP_ID="dev.smix.fixture"
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
  [[ -n "${SERIAL:-}" ]] && SMIX_RUNNER_PORT="$RUNNER_PORT" "$SMIX_BIN" runner down --platform android --device "$SERIAL" \
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

# --runner-port stated, not inherited: this gate's proxy forwards a
# port it chose, and a SMIX_RUNNER_PORT exported for the iOS gates
# (22087 was once occupied by another session's runner) silently moved
# the Android runner while the proxy kept waiting on 28080 — a 180s
# timeout that read as "the runner is broken".
"$SMIX_BIN" runner up "$SERIAL" --platform android --runner-port "$RUNNER_PORT" > "$WORK/runner-up.log" 2>&1 \
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

# A4 — an ordinary app's window is in the tree, and readable.
#
# This is the assertion that was missing when a consumer reported /tree
# carrying the SystemUI windows and not their app's, while every gate
# here was green: not one of them drove an app that was not Settings.
#
# /windows separates the two ways this fails, which look identical from
# the tree alone — a window that is not attached, and a window attached
# with a root the walk cannot read.
FIXTURE_APK="$(bash "$REPO_ROOT/scripts/dev/build-android-fixture.sh" 2>/dev/null)" \
  || die "A4: the fixture app did not build"
adb -s "$SERIAL" install -r "$FIXTURE_APK" >/dev/null 2>&1 \
  || die "A4: could not install the fixture on $SERIAL"
adb -s "$SERIAL" shell am start -n "$FIXTURE_APP_ID/.MainActivity" >/dev/null 2>&1 \
  || die "A4: could not foreground $FIXTURE_APP_ID on $SERIAL"
sleep 3

curl -sS --max-time 15 "http://localhost:$PROXY_PORT/windows" > "$WORK/windows.json" \
  || die "A4: /windows did not answer"
python3 - "$WORK/windows.json" "$FIXTURE_APP_ID" <<'PY' || die "A4: see above. $WORK/windows.json"
import json, sys
doc = json.load(open(sys.argv[1]))
app = sys.argv[2]
rows = doc.get("windows", [])
if not rows:
    print("A4: /windows listed no windows at all — nothing here proves anything",
          file=sys.stderr)
    sys.exit(1)
mine = [r for r in rows if r.get("package") == app]
if not mine:
    seen = sorted({r.get("package") for r in rows})
    print(f"A4: no window belongs to {app}. Attached: {seen}. Its window is not "
          "attached for accessibility — which reads, from /tree alone, exactly "
          "like an app with no accessibility nodes.", file=sys.stderr)
    sys.exit(1)
unreadable = [r for r in mine if not r.get("rootReadable")]
if unreadable:
    print(f"A4: {app} has {len(unreadable)} window(s) attached whose root could "
          "not be read, so they are absent from the tree while present on screen.",
          file=sys.stderr)
    sys.exit(1)
print(f"  A4a: {app} has {len(mine)} readable window(s) among {len(rows)}")
PY

curl -sS --max-time 20 "http://localhost:$PROXY_PORT/tree" > "$WORK/fixture-tree.json" \
  || die "A4: /tree did not answer for the fixture"
python3 - "$WORK/fixture-tree.json" <<'PY' || die "A4: see above. $WORK/fixture-tree.json"
import json, sys
tree = json.load(open(sys.argv[1]))
unreadable = tree.get("unreadableWindows")
def walk(n):
    if "fixture_input" in (n.get("identifier") or ""):
        return True
    return any(walk(c) for c in n.get("children", []))
if not walk(tree):
    print("A4: the fixture's own field is not in the tree while its window is "
          f"attached and readable (unreadableWindows={unreadable})", file=sys.stderr)
    sys.exit(1)
print(f"  A4b: the fixture's field is in the tree (unreadableWindows={unreadable})")
PY

echo "android behaviour gate: 5/5 assertions on $SERIAL ($APP + $FIXTURE_APP_ID)"
