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
# The app the FLOW binds to, which is not always the one we launch.
#
# Tapping Settings' search bar hands the screen to
# `com.android.settings.intelligence`, so the flow declares that as its
# appId and every driving request carries it. A2 asserts the header is
# present on each one, and comparing against $APP made it fail on a
# correct run — the launcher and the flow's subject are two different
# facts. Read from the flow rather than restated here: one of them
# changing without the other is exactly what happened.
FLOW_APP="$(grep -m1 '^appId:' "$SCRIPT_DIR/android-behaviour/force-key-events.yaml" | awk '{print $2}')"
# The second subject. Settings is a system app — preinstalled, stable
# ids, windows owned by the system — so A1–A3 prove things about the
# platform's own app and nothing about anybody else's. A4 drives an
# ordinary one, which is the shape a consumer actually has. Added
# alongside rather than instead: the Settings assertions cover the
# system window layer, which the fixture cannot.
FIXTURE_APP_ID="dev.smix.fixture"
FLOW="$SCRIPT_DIR/android-behaviour/force-key-events.yaml"
# Both are *host* ports. An adb forward claims one for one device, and a
# second emulator on the same machine then finds it answering and refuses
# -- with a message about the app in front, which is not what went wrong.
# That was measured once with an emulator-5556 nobody in this session
# started, and the answer then was to make the port overridable. Nothing
# set the override, so the next bystander holding 28080 killed a ship the
# same way. Ask the OS instead: a port nobody else can be holding is the
# only kind that is one's own.
# shellcheck source=/dev/null
. "$REPO_ROOT/scripts/lib/gate-port.sh"
RUNNER_PORT="${SMIX_GATE_RUNNER_PORT:-$SMIX_RUNNER_PORT}"
PROXY_PORT="${SMIX_GATE_PROXY_PORT:-$(python3 -c 'import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()')}"

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
  # Not the first emulator adb lists — that handed this gate somebody
  # else's device once. pick-dev-emulator asks the ledger whether smix
  # booted a device, and refuses rather than guesses.
  SERIAL="$(bash "$REPO_ROOT/scripts/dev/pick-dev-emulator.sh" 2>&1)" || {
    # The picker already prints how to fix this, in full, to stderr —
    # `$SERIAL` holds that text. Restating it here is what let the two
    # gates drift: the instrumentation gate learned about `lease claim`
    # in 7.0 and this one kept telling people to set an environment
    # variable that records nothing. One place says it now.
    die "no emulator this gate may drive:
$SERIAL"
  }
fi

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
python3 - "$WIRE" "$FLOW_APP" <<'PY' || die "A2: see above. Wire log: $WIRE"
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
#
# Two questions live here and a fixed wait ran them together: whether the
# homepage arrived, and which spelling found the node on it. A slow launch
# made the second one fail and the verdict blamed the spelling. So wait for
# the node to exist first -- bounded, and through the runner port rather
# than the recorder, because this is a precondition and not what A3
# measures -- and say so plainly when it never shows up.
adb -s "$SERIAL" shell am start -a android.settings.SETTINGS >/dev/null 2>&1
PROBE_SEEN=""
for _ in $(seq 1 30); do
  if curl -sS --max-time 5 "http://localhost:$RUNNER_PORT/tree" \
       -H "App-Bundle-Id: $APP" 2>/dev/null | grep -q "\"$PROBE_ID\""; then
    PROBE_SEEN=yes
    break
  fi
  sleep 1
done
[[ -n "$PROBE_SEEN" ]] || die "A3: $PROBE_ID never appeared in the tree of $APP
  within 30s of starting Settings. This is not about spellings -- the screen
  the probe lives on never arrived, so there was nothing to match.
  Current activity:
    $(adb -s "$SERIAL" shell dumpsys activity activities 2>/dev/null | grep -m1 topResumedActivity)"
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

# A window is attached a moment before its root becomes readable, and
# "a moment" is not a constant: idle, it is under a second; nine seconds
# after the instrumentation gate finished driving this same device, it
# was past three. `sleep 3` made that a coin toss and the 7.0.0 ship lost
# it at thirty-one minutes — reporting a product failure where the truth
# was that nothing had waited long enough.
#
# Polling turns the red back into a verdict: not "was unreadable at the
# instant I looked" but "never became readable in A4_SETTLE_S seconds".
A4_SETTLE_S="${SMIX_A4_SETTLE_S:-30}"
a4_deadline=$(( SECONDS + A4_SETTLE_S ))
while :; do
  curl -sS --max-time 15 "http://localhost:$PROXY_PORT/windows" > "$WORK/windows.json" \
    || die "A4: /windows did not answer"
  python3 "$SCRIPT_DIR/android-a4-verdict.py" "$WORK/windows.json" "$FIXTURE_APP_ID" \
    >/dev/null 2>&1 && break
  [ "$SECONDS" -ge "$a4_deadline" ] && break
  sleep 1
done
# The judgement lives in its own file so it can be tested without a
# device: it died here as a TypeError on 2026-08-24 with the ship at
# thirty-one minutes, and a verdict that cannot report its own finding
# is worth less than no verdict. `android-a4-verdict.test.py` drives it
# on that exact payload.
python3 "$SCRIPT_DIR/android-a4-verdict.py" "$WORK/windows.json" "$FIXTURE_APP_ID" \
  || die "A4: see above. $WORK/windows.json"

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

# A5 — a named fill puts the text where it says it did.
#
# A consumer reported `inputText: { id, text }` reporting ok while the
# field stayed empty — the first fill on a screen, every time, with the
# tap-then-type form working on the same screen in the same run.
# Reproduced here before this was written: `smix fill` printed
# "filled id:fixture_input (17 chars)" and the field held nothing.
#
# The verdict reads fixture_result rather than the field, deliberately.
# The field's own value comes back through the same path being tested;
# the label only changes when Submit hands the app what it actually
# received, so it says what arrived rather than what smix believes it
# sent.
adb -s "$SERIAL" shell am force-stop "$FIXTURE_APP_ID" >/dev/null 2>&1
adb -s "$SERIAL" shell am start -n "$FIXTURE_APP_ID/.MainActivity" >/dev/null 2>&1 \
  || die "A5: could not foreground $FIXTURE_APP_ID"
sleep 3

MARKER="a5-first-fill"
"$SMIX_BIN" fill "id:fixture_input" --text "$MARKER" --device "$SERIAL" \
  --port "$PROXY_PORT" > "$WORK/a5-fill.log" 2>&1 \
  || die "A5: the fill itself failed. Log: $WORK/a5-fill.log"
"$SMIX_BIN" tap "id:fixture_submit" --device "$SERIAL" --port "$PROXY_PORT" \
  > "$WORK/a5-submit.log" 2>&1 \
  || die "A5: could not press Submit. Log: $WORK/a5-submit.log"
sleep 1

curl -sS --max-time 20 "http://localhost:$PROXY_PORT/tree" > "$WORK/a5-tree.json" \
  || die "A5: /tree did not answer"
python3 "$REPO_ROOT/scripts/release/android-a5-verdict.py" \
  "$WORK/a5-tree.json" "$MARKER" || die "A5: see above. $WORK/a5-tree.json"

# A6 — hideKeyboard with no keyboard up leaves the screen alone.
#
# On Android there is no first-class hide-keyboard, so the handler
# pressed back. Back with a keyboard up closes the keyboard; back with
# none closes whatever is in front. A consumer reported the failure
# shapes; the worse one is here — with nothing focused it answered
# ok:true and sent the fixture to the launcher, so a flow that dismisses
# a keyboard defensively (iOS needs it, the keyboard covers the next
# button) silently backs out of the app on Android and reports success.
#
# maestro treats it as a no-op when there is no keyboard, and a flow
# written once for both platforms cannot ask "is a keyboard up" first.
adb -s "$SERIAL" shell am force-stop "$FIXTURE_APP_ID" >/dev/null 2>&1
adb -s "$SERIAL" shell am start -n "$FIXTURE_APP_ID/.MainActivity" >/dev/null 2>&1 \
  || die "A6: could not foreground $FIXTURE_APP_ID"
sleep 3

curl -sS --max-time 15 -X POST "http://localhost:$PROXY_PORT/hide-keyboard" \
  -H "App-Bundle-Id: $FIXTURE_APP_ID" -d '{}' > "$WORK/a6-hide.json" 2>&1 \
  || die "A6: /hide-keyboard did not answer"
sleep 2

RESUMED="$(adb -s "$SERIAL" shell dumpsys activity activities 2>/dev/null \
  | grep -m1 'ResumedActivity' || true)"
case "$RESUMED" in
  *"$FIXTURE_APP_ID"*)
    echo "  A6: hideKeyboard with no keyboard up left $FIXTURE_APP_ID in front"
    ;;
  *)
    die "A6: hideKeyboard with no keyboard up took $FIXTURE_APP_ID off the
  screen. It answered $(cat "$WORK/a6-hide.json") and pressed back, which with
  no keyboard to close closes the app instead. Resumed now: $RESUMED"
    ;;
esac

# A7-A8 — the same questions, asked of a Compose screen.
#
# A5 and A6 drove an android.widget.EditText, and 6.4.0's read-back
# predicates were true of that and false of a Compose field: the text
# reaches the accessibility node asynchronously, and the node handed
# over by findFocus carries the fields it had when it was fetched. Both
# read as failures on fills that had worked, and a consumer's entire
# Android suite went red on the release meant to fix its fills.
#
# The subject is the finding. A gate that only ever drives the toolkit
# with the simplest semantics cannot see a predicate that holds only
# there.
adb -s "$SERIAL" shell am start -n "$FIXTURE_APP_ID/.ComposeActivity" >/dev/null 2>&1 \
  || die "A7: could not foreground the Compose screen"
sleep 3

COMPOSE_MARKER="a7-compose-fill"

# A7 — a fill into a Compose field lands, and is read back as landed.
"$SMIX_BIN" fill "id:compose_input" --text "$COMPOSE_MARKER" --device "$SERIAL" \
  --port "$PROXY_PORT" > "$WORK/a7-fill.log" 2>&1 \
  || die "A7: a fill into a Compose field was refused: $(tail -2 "$WORK/a7-fill.log")
  The characters may well be in the field. 6.4.0 refused this exact fill while
  the tree showed all of them there, because the read-back read a node it had
  not refreshed."

curl -sS --max-time 20 "http://localhost:$PROXY_PORT/tree" > "$WORK/a7-tree.json" \
  || die "A7: /tree did not answer after the Compose fill"
python3 "$REPO_ROOT/scripts/release/android-a7-verdict.py" \
  "$WORK/a7-tree.json" "$COMPOSE_MARKER" || die "A7: see above"

# A8 — hideKeyboard with a keyboard up answers ok AND the IME goes.
#
# The fill above focused the field, so the keyboard is up. pressBack()
# reports whether the key event was injected, not whether the keyboard
# went, and a consumer measured it answering false with the IME gone
# from the window list one call later.
curl -sS --max-time 15 "http://localhost:$PROXY_PORT/windows" > "$WORK/a8-before.json" \
  || die "A8: /windows did not answer"
python3 "$REPO_ROOT/scripts/release/android-a8-verdict.py" before "$WORK/a8-before.json" \
  || die "A8: see above"

curl -sS --max-time 15 -X POST "http://localhost:$PROXY_PORT/hide-keyboard" \
  -H "App-Bundle-Id: $FIXTURE_APP_ID" -d '{}' > "$WORK/a8-hide.json" 2>&1 \
  || die "A8: /hide-keyboard did not answer"
grep -q '"ok":true' "$WORK/a8-hide.json" \
  || die "A8: hideKeyboard with a keyboard up answered $(cat "$WORK/a8-hide.json").
  If the IME is gone from /windows, that is the boolean being wrong rather than
  the dismissal failing — which is exactly what a consumer measured."

curl -sS --max-time 15 "http://localhost:$PROXY_PORT/windows" > "$WORK/a8-after.json" \
  || die "A8: /windows did not answer after the dismissal"
python3 "$REPO_ROOT/scripts/release/android-a8-verdict.py" after "$WORK/a8-after.json" \
  || die "A8: see above"

# A9 — a fill into a masked field is judged by what a mask can tell you.
#
# The predicate 6.4.0 shipped compares the node's text with what was
# typed. A password field's node reports one bullet per character, so
# that comparison is false for every fill that ever worked, and a
# consumer's twenty-flow suite stopped at the flow that signs in.
#
# Straight on from A8, with compose_input still filled and the keyboard
# just dismissed. This ran from a fresh screen while the focus handoff
# was broken — the tap landed on the masked field while the clear and
# the typing still went to the previous one — and it is the sequence
# again now that A10 covers that. A9 and A10 back to back are what a
# flow does: fill a field, dismiss the keyboard, fill another.
SECRET='Sunroom!24'
"$SMIX_BIN" fill "id:compose_password" --text "$SECRET" --device "$SERIAL" \
  --port "$PROXY_PORT" > "$WORK/a9-fill.log" 2>&1 \
  || die "A9: a fill into a masked field was refused: $(tail -2 "$WORK/a9-fill.log")
  The characters are very likely in the field. A masked node cannot report them,
  so a predicate that asks for them refuses every such fill."

curl -sS --max-time 20 "http://localhost:$PROXY_PORT/tree" > "$WORK/a9-tree.json" \
  || die "A9: /tree did not answer after the masked fill"
python3 "$REPO_ROOT/scripts/release/android-a9-verdict.py" \
  "$WORK/a9-tree.json" compose_password "$SECRET" 0 || die "A9: see above"

# The count is derived, not typed. It said 9/9 while this file held nine
# assertions and would have gone on saying it while holding ten — a
# number nobody re-counts is a claim with nothing behind it, which is
# the same shape as the defect A9 exists for.
# A10 — a named fill goes to the field it names, and leaves the rest
# alone.
#
# A9 left compose_password focused and holding ten characters. Naming a
# different field now is the case the wait cannot see: it asks whether
# some editable node has focus, and one already does, so it returns the
# old one — the characters land there and the clear that precedes them
# empties it.
"$SMIX_BIN" fill "id:compose_input" --text 'a10-second-field' --device "$SERIAL" \
  --port "$PROXY_PORT" > "$WORK/a10-fill.log" 2>&1 \
  || die "A10: a fill naming a second field was refused: $(tail -2 "$WORK/a10-fill.log")"

curl -sS --max-time 20 "http://localhost:$PROXY_PORT/tree" > "$WORK/a10-tree.json" \
  || die "A10: /tree did not answer after the second fill"
python3 "$REPO_ROOT/scripts/release/android-a10-verdict.py" \
  "$WORK/a10-tree.json" compose_input 'a10-second-field' compose_password 10 \
  || die "A10: see above"

# A11 — a field named by the layout around it can be typed into.
#
# 6.7.1 asked the runner for a focused field containing the tap point.
# A consumer's hand-written Kotlin views carry the contentDescription on
# the wrapper and nothing on the input, so a selector resolves to the
# wrapper — whose centre sits on a label as often as on the field. Every
# fill in that app stopped working, and they reported it against the
# release meant to make fills reliable.
adb -s "$SERIAL" shell am force-stop "$FIXTURE_APP_ID" >/dev/null 2>&1
adb -s "$SERIAL" shell am start -n "$FIXTURE_APP_ID/.MainActivity" >/dev/null 2>&1 \
  || die "A11: could not foreground the view screen"
sleep 2

# Focus somewhere else first: the fill has to reach the field inside
# what was named, not the field that happened to hold focus.
"$SMIX_BIN" tap "id:fixture_input" --device "$SERIAL" --port "$PROXY_PORT" \
  > "$WORK/a11-tap.log" 2>&1 \
  || die "A11: could not focus the first field"

WRAPPED="a11-through-wrapper"
"$SMIX_BIN" fill "text:wrapped-field" --text "$WRAPPED" --device "$SERIAL" \
  --port "$PROXY_PORT" > "$WORK/a11-fill.log" 2>&1 \
  || die "A11: a fill naming the layout around a field was refused: $(tail -2 "$WORK/a11-fill.log")
  The wrapper's centre is not inside the input, which is the shape of every
  field in an app that names them from the outside."

curl -sS --max-time 20 "http://localhost:$PROXY_PORT/tree" > "$WORK/a11-tree.json" \
  || die "A11: /tree did not answer after the wrapped fill"
python3 "$REPO_ROOT/scripts/release/android-a11-verdict.py" \
  "$WORK/a11-tree.json" "$WRAPPED" || die "A11: see above"

# A12 — the keyboard is a thing a flow can wait for, here too.
#
# `role:keyboard` answers on iOS and timed out here, while a screenshot
# showed the QWERTY and /windows listed the IME. The runner knew and the
# tree did not carry it, so no verb could see it — and a consumer,
# reading that as "there is no way to wait for the keyboard", reached for
# a pause. Asked with both expectations, because a tree that never
# carries it would pass the "gone" half forever.
adb -s "$SERIAL" shell am start -n "$FIXTURE_APP_ID/.MainActivity" >/dev/null 2>&1 \
  || die "A12: could not foreground $FIXTURE_APP_ID on $SERIAL"
# `|| true` here would be the bug this gate is about: a setup step that
# silently does nothing leaves A11's keyboard up, and the "absent" half
# then fails for a reason that has nothing to do with what it asks. Ask
# the verb that exists, and let it fail out loud.
"$SMIX_BIN" hide-keyboard --device "$SERIAL" --port "$PROXY_PORT" \
  > "$WORK/a12-hide.log" 2>&1 \
  || die "A12: could not put the keyboard away before asking whether it is gone: $(tail -2 "$WORK/a12-hide.log")"
# The IME window detaches a moment after the dismissal, the same way it
# attaches a moment after the tap.
a12_down_deadline=$(( SECONDS + 15 ))
while :; do
  curl -sS --max-time 20 "http://localhost:$PROXY_PORT/tree" > "$WORK/a12-down.json" || true
  python3 "$REPO_ROOT/scripts/release/android-a12-verdict.py" "$WORK/a12-down.json" absent \
    >/dev/null 2>&1 && break
  [ "$SECONDS" -ge "$a12_down_deadline" ] && break
  sleep 1
done

python3 "$REPO_ROOT/scripts/release/android-a12-verdict.py" "$WORK/a12-down.json" absent \
  || die "A12: see above. $WORK/a12-down.json"

"$SMIX_BIN" tap "id:fixture_input" --device "$SERIAL" --port "$PROXY_PORT" \
  > "$WORK/a12-tap.log" 2>&1 \
  || die "A12: could not focus the fixture field: $(tail -2 "$WORK/a12-tap.log")"
# The window attaches a moment after the tap, the same way A4's does.
a12_deadline=$(( SECONDS + 15 ))
while :; do
  curl -sS --max-time 20 "http://localhost:$PROXY_PORT/tree" > "$WORK/a12-up.json" || true
  python3 "$REPO_ROOT/scripts/release/android-a12-verdict.py" "$WORK/a12-up.json" present \
    >/dev/null 2>&1 && break
  [ "$SECONDS" -ge "$a12_deadline" ] && break
  sleep 1
done
python3 "$REPO_ROOT/scripts/release/android-a12-verdict.py" "$WORK/a12-up.json" present \
  || die "A12: see above. $WORK/a12-up.json"

# A13 — a swipe aimed inside a named element reaches this platform too.
#
# Weaker than its iOS counterpart, and deliberately so rather than by
# omission. `keyboard-comes-and-goes.yaml` asserts an effect — a row far
# enough down to be off-screen at rest is on screen after two swipes
# inside the list — and this fixture has no scrollable list to make that
# assertion against.
#
# What differs by platform here is only the driver's swipe dispatch: the
# resolution and the arithmetic that turns shares of a box into points
# are host-side and shared. So this asks the part that is actually
# per-platform — the element resolves against the Android tree and the
# swipe goes — and the control asks the failure that matters, which is
# that an element it cannot find produces no swipe at all rather than a
# sweep across the middle of the screen.
# A file rather than `/dev/stdin`: a heredoc is an fd the child cannot
# reopen by name, and `smix run` takes a path.
cat > "$WORK/a13-over.yaml" <<'A13FLOW'
appId: dev.smix.fixture
---
# The assertions before this one leave the app wherever they left it, so
# this says what it wants in front — and then waits for it. `launchApp`
# returning is not the tree being ready on Android: a flow that asserts
# straight after it fails with the element not found while `/tree` a
# moment later has it. That is its own defect (open-items B9) and not
# this assertion's subject, so this waits the way the iOS corpus flow
# does rather than racing it.
- launchApp
- extendedWaitUntil:
    visible: { id: "fixture_input" }
    timeout: 10000
- swipe:
    over: { id: "fixture_input" }
    from: 0.2
    to: 0.8
A13FLOW
# Through the proxy, like every other driving call here: the rest of
# this gate passes --port "$PROXY_PORT", and a call that does not
# reaches a different endpoint with a different view of the app.
"$SMIX_BIN" run "$WORK/a13-over.yaml" --platform android --device "$SERIAL" \
  --port "$PROXY_PORT" > "$WORK/a13-over.log" 2>&1
if [ $? -ne 0 ]; then
  die "A13: a swipe over a named element failed: $(tail -3 "$WORK/a13-over.log")"
fi
echo "  A13a: a swipe aimed inside fixture_input reached the device"

cat > "$WORK/a13-absent.yaml" <<'A13ABSENT'
appId: dev.smix.fixture
---
- launchApp
- extendedWaitUntil:
    visible: { id: "fixture_input" }
    timeout: 10000
- swipe:
    over: { id: "no-such-element" }
    from: 0.2
    to: 0.8
A13ABSENT
# Through the proxy, like every other driving call here: the rest of
# this gate passes --port "$PROXY_PORT", and a call that does not
# reaches a different endpoint with a different view of the app.
"$SMIX_BIN" run "$WORK/a13-absent.yaml" --platform android --device "$SERIAL" \
  --port "$PROXY_PORT" > "$WORK/a13-absent.log" 2>&1
if [ $? -eq 0 ]; then
  die "A13: a swipe over an element that is not there reported success. \
Falling back to shares of the screen would swipe across whatever is showing \
and call the step done — which is the form \`over:\` exists to replace."
fi
echo "  A13b: an element that is not there is refused, not swept over"

ASSERTIONS=$(grep -cE '^# A[0-9]+( control)? —' "$0")
echo "android behaviour gate: $ASSERTIONS/$ASSERTIONS assertions on $SERIAL ($APP + $FIXTURE_APP_ID)"
