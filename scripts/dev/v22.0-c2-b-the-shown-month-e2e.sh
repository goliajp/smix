#!/usr/bin/env bash
# v22.0-C2: `text: "1"` in the platform's date picker finds the shown month's
# first day, on a tablet.
#
# The picker keeps the months beside the one it shows in its tree, each
# page reported hidden and its days reported visible. On a phone those
# pages' boxes fall off the screen; centred on a tablet they do not, and
# `tapOn: "1"` aimed at the previous month's first day. A candidate under a
# hidden ancestor now yields to one that is not.
#   1. pick the first: the app receives the shown month's first day
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/judged-run.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port PORT
ALIAS="$E2E_ANDROID_TABLET"
APP="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
WORK="$(mktemp -d)"

log()  { printf '[v22.0-c2-b] %s\n' "$*" >&2; }
fail() { printf '[v22.0-c2-b] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v22.0-c2-b] cannot judge: %s\n' "$*" >&2; exit 2; }

DEVICE="" UP=0 WE_BOOTED=0
cleanup() {
  local said
  if [ "$UP" = 1 ] \
    && ! said="$("$SMIX" runner down --platform android --device "$DEVICE" --runner-port "$PORT" 2>&1)"; then
    log "warning: the runner on $PORT was not stopped:"
    printf '%s\n' "$said" | tail -3 >&2
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$DEVICE" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

flow() { # $1 name, rest: steps
  local name="$1"; shift
  { printf 'appId: %s\n---\n' "$APP"; printf '%s\n' "$@"; } >"$WORK/$name.yaml"
  "$SMIX_RUN" --device "$DEVICE" --runner-port "$PORT" "$WORK/$name.yaml" >"$WORK/$name.log" 2>&1 \
    || { tail -15 "$WORK/$name.log" >&2; fail "flow '$name' failed"; }
}

tree_json() {
  "$SMIX" tree --device "$DEVICE" --runner-port "$PORT" --json 2>/dev/null
}

label() { # the text of the element with this id
  tree_json | python3 -c '
import json, sys
want = sys.argv[1]
def walk(n):
    if n.get("identifier") == want:
        print(n.get("text") or n.get("label") or ""); sys.exit(0)
    for c in n.get("children", []): walk(c)
walk(json.load(sys.stdin)["root"])' "$1"
}

is() { # $1 id, $2 expected text, $3 step
  local got
  got="$(label "$1")"
  [ "$got" = "$2" ] || fail "$3: $1 reads '$got', expected '$2'"
}

command -v adb >/dev/null 2>&1 || cannot_judge "no adb"
[ -f "$APK" ] || fail "no fixture apk (bash scripts/dev/build-android-fixture.sh)"
python3 "$ROOT/scripts/dev/fixture-apk-stamp.py" --check >&2 \
  || fail "the fixture apk on disk is not the one this tree builds"
if ! DEVICE="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tail -1)" || [ -z "$DEVICE" ]; then
  with_deadline 900 "$SMIX" sim boot "$ALIAS" --wait 600 --no-window >"$WORK/boot.log" 2>&1 \
    || cannot_judge "$ALIAS could not be booted: $(tail -3 "$WORK/boot.log")"
  DEVICE="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tail -1)" || fail "$ALIAS was booted and does not resolve"
  WE_BOOTED=1
fi
case "$DEVICE" in
  emulator-*) : ;;
  *) fail "$ALIAS resolves to '$DEVICE', which is not an emulator — refusing" ;;
esac
for _ in $(seq 1 60); do
  adb -s "$DEVICE" shell getprop sys.boot_completed 2>/dev/null | grep -q 1 && break
  sleep 2
done
adb -s "$DEVICE" install -r -g "$APK" >/dev/null 2>&1 || fail "could not install the fixture"
"$SMIX" runner up "$DEVICE" --platform android --runner-port "$PORT" >"$WORK/up.log" 2>&1 \
  || fail "runner up: $(tail -5 "$WORK/up.log")"
UP=1

adb -s "$DEVICE" shell am start -W -S -n "$APP/.NativeDialogActivity" >/dev/null 2>&1 \
  || fail "could not start the dialog screen"

log "1. tapOn \"1\" in the date picker picks the shown month's first day"
flow pick \
  '- tapOn: { id: native_date_open }' \
  '- extendedWaitUntil: { visible: "OK", timeout: 5000 }' \
  '- tapOn: "1"' \
  '- tapOn: "OK"' \
  '- assertVisible: { id: native_date_picked, text: "picked 2026-11-01" }'

log "V22.0-C2-B-THE-SHOWN-MONTH-PASS"
