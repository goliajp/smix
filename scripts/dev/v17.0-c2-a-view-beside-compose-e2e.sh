#!/usr/bin/env bash
# v17.0-C2: a View the app lays over its Compose screen is in the tree.
#
# A consumer's app adds an environment badge — a FrameLayout and a
# TextView on `android.R.id.content` — over a screen built with a
# ComposeView. The accessibility dump listed it; smix's tree, `find text:`
# and `label:` did not, because the probe answered a window that holds a
# Compose root with that root alone. The fixture's badge screen has the
# same shape.
#
# Judged through the probe's own tree and through `smix find`: the badge
# is there by its text and by its description, and the Compose content
# beside it is still there (a tree that lost everything would also not
# list the badge).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port PORT
ALIAS="${SMIX_C2_ANDROID:-$E2E_ANDROID}"
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
BADGE="IDENTITY.STAGING.EXAMPLE.COM"
WORK="$(mktemp -d)"

log()  { printf '[v17.0-c2] %s\n' "$*" >&2; }
fail() { printf '[v17.0-c2] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v17.0-c2] cannot judge: %s\n' "$*" >&2; exit 2; }

SERIAL="" WE_BOOTED=0 WE_UPPED=0
cleanup() {
  if [ "$WE_UPPED" = 1 ]; then
    if ! said="$("$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" 2>&1)"; then
      log "warning: the runner was not stopped:"
      printf '%s\n' "$said" | tail -3 >&2
    fi
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

command -v adb >/dev/null 2>&1 || cannot_judge "no adb"
[ -f "$APK" ] || fail "no fixture apk (bash scripts/dev/build-android-fixture.sh)"
python3 "$ROOT/scripts/dev/fixture-apk-stamp.py" --check >&2 \
  || fail "the fixture apk on disk is not the one this tree builds"
if ! SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tail -1)" || [ -z "$SERIAL" ]; then
  with_deadline 300 "$SMIX" sim boot "$ALIAS" >"$WORK/boot.log" 2>&1 || fail "boot: $(tail -3 "$WORK/boot.log")"
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
"$SMIX" runner up "$SERIAL" --platform android --runner-port "$PORT" >"$WORK/up.log" 2>&1 \
  || fail "runner up: $(tail -5 "$WORK/up.log")"
WE_UPPED=1
adb -s "$SERIAL" shell am start -W -S -n "$APPID/.BadgeActivity" >/dev/null 2>&1 \
  || fail "could not start the badge screen"

probe_tree() {
  curl -s -m 30 -H "App-Bundle-Id: $APPID" "http://localhost:$PORT/probe/tree?app=$APPID"
}
seen=""
for _ in $(seq 1 20); do
  t="$(probe_tree)"
  if printf '%s' "$t" | grep -q 'badge-landing-button'; then seen="$t"; break; fi
  sleep 1
done
[ -n "$seen" ] || cannot_judge "the probe never listed the Compose button — this is not the badge screen, or the probe is not in the app"
printf '%s' "$seen" | grep -q "\"$BADGE\"" \
  || fail "the probe's tree has the Compose content and not the View laid over it"
log "the probe lists the Compose button and the badge beside it"

find_one() { # $1 selector
  "$SMIX" find "$1" --device "$SERIAL" --runner-port "$PORT" >"$WORK/find.log" 2>&1 \
    || { tail -5 "$WORK/find.log" >&2; fail "smix find $1 did not find it"; }
}
find_one "text:$BADGE"
find_one "label:environment $BADGE"
find_one "id:badge-landing-button"
log "smix find answers the badge by its text and its description, and the button beside it"
echo "VIEW-BESIDE-COMPOSE-PASS"
