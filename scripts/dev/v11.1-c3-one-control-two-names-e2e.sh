#!/usr/bin/env bash
# One control, the two names two phones give it.
#
# A consumer's player has a middle button with an icon and nothing else:
# on Android its name is a contentDescription, which smix reaches as
# `label`; on iOS the same control's name is its accessibilityLabel. A
# flow that runs on both phones wants "this control, under either name",
# which is what `fallback` is for — and the chain could not hold `label`,
# because it had a hand-kept parser of its own.
#
# This measures, on both platforms, where an icon-only control's name
# lands and which way of writing it finds it, and presses it each way.
# The verdict is read from the app's own count, never from smix having
# said "tapped":
#
#   * `fallback: [text, label]` must press the control, every time, on
#     both platforms and for both kinds of Android control;
#   * `text:` and `label:` alone are measured and printed as a table —
#     the consumer reported `text:` missing where `label:` found it, and
#     the code says it should not; this is where that is settled.
#
# Exit: 0 judged and passed, 1 judged and failed, 2 could not judge.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
AND_PORT="$SMIX_RUNNER_PORT"
gate_free_port IOS_PORT
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../lib" && pwd)/e2e-devices.sh"
AND_ALIAS="${SMIX_C3_ANDROID:-$E2E_ANDROID}"
IOS_ALIAS="${SMIX_C3_IOS:-$E2E_IOS}"
AND_APPID="dev.smix.fixture"
IOS_APPID="jp.golia.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
APP="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
WORK="$(mktemp -d)"

log()  { printf '[c3-names] %s\n' "$*" >&2; }
fail() { printf '[c3-names] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[c3-names] CANNOT JUDGE: %s\n' "$*" >&2; exit 2; }

SERIAL="" UDID="" AND_UPPED=0 IOS_UPPED=0 WE_BOOTED=0 IOS_WE_BOOTED=0
cleanup() {
  local said
  if [ "$AND_UPPED" = 1 ]; then
    said="$("$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$AND_PORT" 2>&1)" \
      || printf '[c3-names] warning: the Android runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
  fi
  if [ "$IOS_UPPED" = 1 ]; then
    said="$("$SMIX" runner down --device "$UDID" --runner-port "$IOS_PORT" 2>&1)" \
      || printf '[c3-names] warning: the iOS runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$AND_ALIAS" >/dev/null 2>&1 || true; fi
  if [ "$IOS_WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

command -v adb >/dev/null 2>&1 || cannot_judge "no adb on PATH"
[ -f "$APK" ] || cannot_judge "no fixture apk — run: bash scripts/dev/build-android-fixture.sh"
[ -d "$APP" ] || cannot_judge "no iOS fixture — run: bash scripts/dev/build-fixture-app.sh"
python3 "$ROOT/scripts/dev/fixture-apk-stamp.py" --check >&2 \
  || fail "the fixture apk on disk is not the one this tree builds"

# The count a control has received, read from the app's own text. $1 is
# the device, $2 the port, $3 the identifier suffix of the count's text.
count_of() {
  local tree
  tree="$("$SMIX" tree --device "$1" --port "$2" --reader a11y --json 2>/dev/null)" \
    || return 1
  TREE_JSON="$tree" WANT="$3" python3 - <<'PY'
import json, os, re, sys
d = json.loads(os.environ["TREE_JSON"])
want = os.environ["WANT"]
def walk(n):
    if (n.get("identifier") or "").split("/")[-1] == want:
        m = re.search(r"(\d+)", " ".join(str(n.get(k) or "") for k in ("text", "label", "value")))
        if m:
            print(m.group(1)); sys.exit(0)
    for c in n.get("children", []):
        walk(c)
walk(d["root"])
sys.exit(1)
PY
}

# Which fields of the tree carry the control's name. $1 device, $2 port,
# $3 reader, $4 name.
fields_of() {
  local tree
  tree="$("$SMIX" tree --device "$1" --port "$2" --reader "$3" --json 2>/dev/null)" \
    || { echo "unreadable"; return 0; }
  TREE_JSON="$tree" NAME="$4" python3 - <<'PY'
import json, os
d = json.loads(os.environ["TREE_JSON"])
name = os.environ["NAME"]
found = set()
def walk(n):
    for k in ("label", "text", "title", "value", "placeholderValue", "identifier"):
        if (n.get(k) or "") == name:
            found.add(k)
    for c in n.get("children", []):
        walk(c)
walk(d["root"])
print(",".join(sorted(found)) or "nowhere")
PY
}

# One press through a flow. $1 platform, $2 device, $3 port, $4 appId,
# $5 selector yaml (one line), $6 count id. Prints the delta.
press_delta() {
  local before after flow rc=0
  before="$(count_of "$2" "$3" "$6")" || { echo "unreadable"; return 0; }
  flow="$WORK/press-$RANDOM.yaml"
  printf 'appId: %s\n---\n- tapOn: %s\n' "$4" "$5" > "$flow"
  SMIX_RUNNER_PORT="$3" "$SMIX" run --device "$2" "$flow" >"$WORK/run.log" 2>&1 || rc=$?
  sleep 1
  after="$(count_of "$2" "$3" "$6")" || { echo "unreadable"; return 0; }
  echo "$(( after - before )) (run exit $rc)"
}

# $1 device, $2 port, $3 name
found_by() {
  local out
  out="$("$SMIX" find --device "$1" --port "$2" "$3" 2>/dev/null)" || true
  case "$out" in *"exists=true"*|*'"exists":true'*) echo yes ;; *) echo no ;; esac
}

FAILED=0
row() { # platform control reader-fields text-find label-find text-press label-press fallback-press
  printf '[c3-names] | %-8s | %-18s | %-22s | %-3s | %-3s | %-14s | %-14s | %-14s |\n' "$@" >&2
}

# ---------------------------------------------------------------- Android
if ! SERIAL="$("$SMIX" sim resolve "$AND_ALIAS" 2>/dev/null | tail -1)" \
   || [ -z "$SERIAL" ] || ! adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1; then
  # By alias, so the ledger records smix booted it and the AVD it is,
  # and only then is the serial read: a serial is the port it happened to
  # take this time.
  log "booting $AND_ALIAS"
  "$SMIX" sim boot "$AND_ALIAS" >/dev/null 2>&1 || cannot_judge "could not boot $AND_ALIAS"
  WE_BOOTED=1
  SERIAL="$("$SMIX" sim resolve "$AND_ALIAS" 2>/dev/null | tail -1)"
fi
case "$SERIAL" in
  emulator-*) : ;;
  *) cannot_judge "$AND_ALIAS resolves to '$SERIAL', which is not an emulator — refusing" ;;
esac
adb -s "$SERIAL" wait-for-device
for _ in $(seq 1 60); do
  adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1 && break
  sleep 2
done
adb -s "$SERIAL" install -r -g "$APK" >/dev/null 2>&1 || fail "could not install the fixture on $SERIAL"
"$SMIX" runner up "$SERIAL" --platform android --runner-port "$AND_PORT" >/dev/null 2>&1 \
  || cannot_judge "the Android runner would not start on $SERIAL:$AND_PORT"
AND_UPPED=1
adb -s "$SERIAL" shell am start -W -n "$AND_APPID/.IconActivity" >/dev/null 2>&1 \
  || fail "could not start the icon screen on $SERIAL"
sleep 2

log "--- Android ($SERIAL)"
row platform control "fields a11y / probe" txt lbl "text: press" "label: press" "fallback press"
for spec in "Pause:icon_pause_count:Compose" "Stop:icon_stop_count:AndroidView"; do
  name="${spec%%:*}"; rest="${spec#*:}"; count_id="${rest%%:*}"; kind="${rest#*:}"
  fa="$(fields_of "$SERIAL" "$AND_PORT" a11y "$name")"
  fp="$(fields_of "$SERIAL" "$AND_PORT" probe "$name")"
  tf="$(found_by "$SERIAL" "$AND_PORT" "text:$name")"
  lf="$(found_by "$SERIAL" "$AND_PORT" "label:$name")"
  tp="$(press_delta android "$SERIAL" "$AND_PORT" "$AND_APPID" "{ text: \"$name\" }" "$count_id")"
  lp="$(press_delta android "$SERIAL" "$AND_PORT" "$AND_APPID" "{ label: \"$name\" }" "$count_id")"
  fb="$(press_delta android "$SERIAL" "$AND_PORT" "$AND_APPID" \
        "{ fallback: [ { text: \"$name\" }, { label: \"$name\" } ] }" "$count_id")"
  row android "$name ($kind)" "$fa / $fp" "$tf" "$lf" "$tp" "$lp" "$fb"
  case "$fb" in
    "1 "*) : ;;
    *) printf '[c3-names] FAIL: android %s (%s): `fallback: [text, label]` pressed it %s times — see %s\n' \
         "$name" "$kind" "$fb" "$(tail -3 "$WORK/run.log" | tr '\n' ' ')" >&2; FAILED=1 ;;
  esac
done

# ---------------------------------------------------------------- iOS
if ! UDID="$("$SMIX" sim resolve "$IOS_ALIAS" 2>/dev/null | tail -1)" || [ -z "$UDID" ]; then
  cannot_judge "no iOS simulator registered as $IOS_ALIAS"
fi
if ! xcrun simctl list devices -j | python3 -c "import json,sys;d=json.load(sys.stdin);sys.exit(0 if any(x['udid']=='$UDID' and x['state']=='Booted' for r in d['devices'].values() for x in r) else 1)"; then
  log "booting $IOS_ALIAS"
  "$SMIX" sim boot "$UDID" >/dev/null 2>&1 || cannot_judge "could not boot $IOS_ALIAS"
  IOS_WE_BOOTED=1
fi
"$SMIX" sim install "$UDID" "$APP" >/dev/null 2>&1 || fail "could not install the iOS fixture on $UDID"
"$SMIX" runner up "$UDID" --bundle "$IOS_APPID" --runner-port "$IOS_PORT" >/dev/null 2>&1 \
  || cannot_judge "the iOS runner would not start on $UDID:$IOS_PORT"
IOS_UPPED=1
printf 'appId: %s\n---\n- launchApp\n' "$IOS_APPID" > "$WORK/ios-open.yaml"
SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX" run --device "$UDID" "$WORK/ios-open.yaml" >"$WORK/ios-open.log" 2>&1 \
  || fail "could not open the iOS fixture: $(tail -3 "$WORK/ios-open.log" | tr '\n' ' ')"
sleep 2

log "--- iOS ($UDID)"
name="Pause"
fa="$(fields_of "$UDID" "$IOS_PORT" a11y "$name")"
tf="$(found_by "$UDID" "$IOS_PORT" "text:$name")"
lf="$(found_by "$UDID" "$IOS_PORT" "label:$name")"
tp="$(press_delta ios "$UDID" "$IOS_PORT" "$IOS_APPID" "{ text: \"$name\" }" fixture-icon-count)"
lp="$(press_delta ios "$UDID" "$IOS_PORT" "$IOS_APPID" "{ label: \"$name\" }" fixture-icon-count)"
fb="$(press_delta ios "$UDID" "$IOS_PORT" "$IOS_APPID" \
      "{ fallback: [ { text: \"$name\" }, { label: \"$name\" } ] }" fixture-icon-count)"
row ios "$name (SwiftUI)" "$fa / -" "$tf" "$lf" "$tp" "$lp" "$fb"
case "$fb" in
  "1 "*) : ;;
  *) printf '[c3-names] FAIL: ios %s: `fallback: [text, label]` pressed it %s times — see %s\n' \
       "$name" "$fb" "$(tail -3 "$WORK/run.log" | tr '\n' ' ')" >&2; FAILED=1 ;;
esac

[ "$FAILED" = 0 ] || exit 1
log "C3-TWO-NAMES-E2E-PASS (a fallback chain of the two names pressed every icon-only control, read from the app's own counts)"
