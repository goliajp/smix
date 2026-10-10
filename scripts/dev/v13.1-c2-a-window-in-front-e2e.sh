#!/usr/bin/env bash
# v13.1-C2: `runner up` names a window that stands in front of the app.
#
# A consumer's `runner up` met a password manager's "save password?" prompt
# and a keyboard's stylus introduction in front of their app. The runner's
# window list then held system UI and one focused window of another
# package, and nothing of the app; `runner up` waited out its settle and
# blamed a crashed instrumentation. 13.0.0 names the window — and until this
# gate that was proved only by unit tests over window lists typed by hand,
# because the state could not be made here.
#
# The fixture makes it now: an opaque full-screen overlay holding the focus
# leaves the app's own window with nothing showing, and the accessibility
# service drops it from the list. The state is read from the runner before
# anything is judged by it.
#
# There is no SKIP path: a gate that cannot judge must be red.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
ALIAS="${SMIX_V131_C2_ANDROID:-$E2E_ANDROID}"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
# Three seconds of the window holding still, and a bring-up around it; the
# old answer took the whole twenty-second settle and then blamed the runner.
WITHIN=15

log()  { printf '[v13.1-c2] %s\n' "$*" >&2; }
fail() { printf '[v13.1-c2] FAIL: %s\n' "$*" >&2; exit 1; }

SERIAL="" WE_BOOTED=0 WORK="" OVERLAY_WAS=""
cleanup() {
  if [ -n "$SERIAL" ]; then
    adb -s "$SERIAL" shell am force-stop "$APPID" >/dev/null 2>&1 || true
    if [ -n "$OVERLAY_WAS" ]; then
      adb -s "$SERIAL" shell appops set "$APPID" SYSTEM_ALERT_WINDOW "$OVERLAY_WAS" >/dev/null 2>&1 \
        || printf '[v13.1-c2] warning: SYSTEM_ALERT_WINDOW was not put back to %s\n' "$OVERLAY_WAS" >&2
    fi
    "$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" \
      >/dev/null || printf '[v13.1-c2] warning: the runner was not stopped\n' >&2
  fi
  [ -n "$WORK" ] && rm -rf "$WORK"
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
}
trap cleanup EXIT

command -v adb >/dev/null 2>&1 || fail "no adb on PATH — this judges an Android runner and cannot"
[ -x "$SMIX" ] || fail "no smix binary at $SMIX (cargo build -p smix-cli)"
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
  # This puts an overlay over the screen; a registered phone belongs to somebody.
  *) fail "$ALIAS resolves to '$SERIAL', which is not an emulator — refusing" ;;
esac
with_deadline 120 adb -s "$SERIAL" wait-for-device || fail "$SERIAL did not come up"
for _ in $(seq 1 60); do
  adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1 && break
  sleep 2
done

WORK="$(mktemp -d)"
adb -s "$SERIAL" install -r -g "$APK" >/dev/null 2>&1 || fail "could not install the fixture"
OVERLAY_WAS="$(adb -s "$SERIAL" shell appops get "$APPID" SYSTEM_ALERT_WINDOW | tr -d '\r' \
  | sed -n 's/.*SYSTEM_ALERT_WINDOW: \([a-z]*\).*/\1/p')"
[ -n "$OVERLAY_WAS" ] || OVERLAY_WAS=default
adb -s "$SERIAL" shell appops set "$APPID" SYSTEM_ALERT_WINDOW allow || fail "could not allow the fixture an overlay"

# A runner that answers before the overlay goes up: the already-running path.
"$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" >/dev/null \
  || log "no runner of ours was answering on $PORT"
"$SMIX" runner up "$ALIAS" --platform android --runner-port "$PORT" >/dev/null 2>&1 \
  || fail "the runner would not start on $SERIAL:$PORT before the overlay"
adb -s "$SERIAL" shell am start -W -S -n "$APPID/.OverlayActivity" >/dev/null 2>&1 \
  || fail "could not start the overlay screen"

# The premise, read from the runner: nothing of any app readable, and a
# focused window of a package that is not system UI.
premise=""
for _ in $(seq 1 20); do
  curl -s -m 10 "http://localhost:$PORT/windows" >"$WORK/windows.json" || true
  if premise="$(python3 - "$WORK/windows.json" "$APPID" <<'PY'
import json, sys
rows = json.load(open(sys.argv[1]))["windows"]
apps = [r for r in rows if r.get("type") == 1 and r.get("rootReadable")]
front = [r for r in rows if r.get("focused") and r.get("package") == sys.argv[2]]
if apps or not front:
    sys.exit(1)
print(", ".join(f"{r.get('package')}(type {r.get('type')}{', focused' if r.get('focused') else ''})" for r in rows))
PY
)"; then break; fi
  sleep 0.5
done
[ -n "$premise" ] || fail "the overlay did not leave the screen as wanted — the runner's windows: $(cat "$WORK/windows.json")"
log "the screen: $premise"

judge() {
  local path="$1" began rc=0 took
  began=$(date +%s)
  "$SMIX" runner up "$ALIAS" --platform android --runner-port "$PORT" >"$WORK/$path.log" 2>&1 || rc=$?
  took=$(( $(date +%s) - began ))
  [ "$rc" -ne 0 ] || fail "$path: runner up reported success with nothing of the app on the screen"
  grep -q "$APPID" "$WORK/$path.log" && grep -qi "holds the focus" "$WORK/$path.log" \
    || { cat "$WORK/$path.log" >&2; fail "$path: the refusal does not name the window in front"; }
  if grep -q "crashed-and-restarted" "$WORK/$path.log"; then
    cat "$WORK/$path.log" >&2; fail "$path: the refusal still blames the instrumentation"
  fi
  if grep -q -- "--force" "$WORK/$path.log"; then
    cat "$WORK/$path.log" >&2; fail "$path: the refusal offers --force, which cannot move the window"
  fi
  [ "$took" -le "$WITHIN" ] || fail "$path: the refusal took ${took}s"
  log "  $path: refused in ${took}s, naming $APPID"
}

log "--- a runner already answering"
judge already-up
"$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" >/dev/null \
  || log "no runner of ours was answering on $PORT"
log "--- a fresh bring-up"
judge fresh

log "--- and with the overlay gone"
adb -s "$SERIAL" shell am force-stop "$APPID"
"$SMIX" runner up "$ALIAS" --platform android --runner-port "$PORT" >"$WORK/after.log" 2>&1 \
  || { tail -5 "$WORK/after.log" >&2; fail "the runner would not come up once the overlay had gone"; }
log "  the runner came up"

log "V13.1-C2-E2E-PASS on $SERIAL"
