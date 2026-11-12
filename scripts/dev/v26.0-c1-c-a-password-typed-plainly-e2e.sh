#!/usr/bin/env bash
# v26.0-C1: a password typed with plain inputText stays out of what a run writes.
#
# `inputText: { secret: … }` keeps a value out of every report. A password
# typed as plain `inputText` did not get that, and it surfaced anyway: the
# keyboard offered it back in its suggestion strip, and the tree saved with
# the next failure carried it. On the fixture's Compose password field:
#   type a password with plain inputText, then fail an assertion, with
#   --format json and --debug-output; the value appears nowhere, and the
#   keyboard's suggestion that held it is written as ***
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port PORT
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
WORK="$(mktemp -d)"
# a word the keyboard offers back; a string of letters and digits it does not
VALUE="Hunter2Secret"

log()  { printf '[v26.0-c1-c] %s\n' "$*" >&2; }
fail() { printf '[v26.0-c1-c] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v26.0-c1-c] cannot judge: %s\n' "$*" >&2; exit 2; }

SERIAL="" WE_BOOTED=0 UP=0
cleanup() {
  local said
  if [ "$UP" = 1 ] && ! said="$("$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" 2>&1)"; then
    log "warning: the runner on $PORT was not stopped:"
    printf '%s\n' "$said" | tail -3 >&2
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

[ -f "$APK" ] || fail "no fixture apk — run: bash scripts/dev/build-android-fixture.sh"
SERIAL="$("$SMIX" sim resolve "$E2E_ANDROID" 2>/dev/null | tr -d '[:space:]')" || true
if [ -z "$SERIAL" ]; then
  "$SMIX" sim boot "$E2E_ANDROID" >"$WORK/boot.log" 2>&1 || fail "emulator boot: $(tail -3 "$WORK/boot.log")"
  WE_BOOTED=1
  SERIAL="$("$SMIX" sim resolve "$E2E_ANDROID" | tr -d '[:space:]')"
fi
case "$SERIAL" in
  emulator-*) ;;
  *) fail "'$E2E_ANDROID' resolves to $SERIAL, which is not an emulator" ;;
esac
"$SMIX" sim install "$SERIAL" "$APK" >"$WORK/install.log" 2>&1 || fail "install: $(tail -3 "$WORK/install.log")"
ANDROID_SERIAL="$SERIAL" "$SMIX" runner up "$SERIAL" --platform android --runner-port "$PORT" \
  >"$WORK/up.log" 2>&1 || fail "runner up: $(tail -5 "$WORK/up.log")"
UP=1

cat >"$WORK/flow.yaml" <<FLOW
appId: $APPID
---
- launchApp:
    clearState: true
- tapOn: { label: "open-compose" }
- tapOn: { id: "compose_password" }
- inputText: "$VALUE"
- assertVisible: "words this screen does not have"
FLOW
rc=0
# raw run: the failure is the subject; what it wrote is judged below
SMIX_RUNNER_PORT="$PORT" "$SMIX" run "$WORK/flow.yaml" --device "$SERIAL" --platform android \
  --runner-port "$PORT" --format json --debug-output "$WORK/dbg" >"$WORK/out.txt" 2>&1 || rc=$?
[ "$rc" = 3 ] || fail "the assertion was to fail (exit 3), got $rc: $(tail -3 "$WORK/out.txt")"
[ -s "$WORK/dbg/step-5-assertvisible.fail.tree.json" ] || fail "no tree saved with the failure"

# The premise: the keyboard offered the typed word back. Without it there
# is nothing on screen for the value to leak through.
offered="$(python3 - "$WORK/dbg/step-5-assertvisible.fail.tree.json" <<'PY'
import json, sys
t = json.load(open(sys.argv[1]))
def walk(n, inside=False):
    inside = inside or n.get("identifier") == "softkey_holder_fixed_candidates"
    yield inside, n
    for c in n.get("children", []):
        yield from walk(c, inside)
print(sum(1 for inside, n in walk(t) if inside and n.get("label") == "***"))
PY
)"
[ "$offered" -gt 0 ] || cannot_judge "the keyboard offered nothing back in its suggestion strip, so nothing could leak"

# grep finds nothing with status 1, which is the answer hoped for
leaks="$({ grep -rl -- "$VALUE" "$WORK/out.txt" "$WORK/dbg" || true; } | wc -l | tr -d ' ')"
[ "$leaks" = 0 ] || fail "the password typed with plain inputText is in $leaks file(s) the run wrote"
log "ok    the keyboard offered the password back ($offered node(s)), written as ***; the value is in nothing the run wrote"
log "PASS"
