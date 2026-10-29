#!/usr/bin/env bash
# v19.0-C2: the time a flow spends that is smix's, not the app's.
#
# A consumer attributed 45 minutes of 22 iOS flows request by request:
# half went to waits, a seventh to fills sent one character per request,
# and a sixth to taps whose settle they could not tune. On the fixture's
# UIKit sign-in step, judged by what the runner was asked and what the
# app shows:
#   1. a named fill of 21 characters is one POST /fill, and the field
#      holds exactly the text
#   2. a tap told to settle on one reading spends no time settling; by
#      default it spends at least one reading's wait
#   3. `--give-up-when-still-ms` ends a 30 s wait for an element that
#      never comes once the screen has stood still, and says so
#   4. three flows in one `smix run` take less than the same three run
#      one by one
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/judged-run.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
IOS_ALIAS="${SMIX_C2_IOS:-$E2E_IOS}"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port IOS_PORT
APPID="jp.golia.smix.fixture"
FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
PROJECT="${SMIX_C2_RUNNER_PROJECT:-$ROOT/swift-bridge/SmixRunner.xcodeproj}"
TEXT="probe.c2@example.test"
WORK="$(mktemp -d)"

log()  { printf '[v19.0-c2] %s\n' "$*" >&2; }
fail() { printf '[v19.0-c2] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v19.0-c2] cannot judge: %s\n' "$*" >&2; exit 2; }

UDID="" WE_BOOTED=0 WE_UPPED=0
cleanup() {
  if [ "$WE_UPPED" = 1 ]; then
    if ! said="$("$SMIX" runner down --device "$UDID" --runner-port "$IOS_PORT" 2>&1)"; then
      printf '[v19.0-c2] warning: the iOS runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
    fi
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

command -v xcrun >/dev/null 2>&1 || cannot_judge "no xcrun"
UDID="$("$SMIX" sim resolve "$IOS_ALIAS" 2>/dev/null | tr -d '[:space:]')" || true
[ -n "$UDID" ] || cannot_judge "no simulator resolves '$IOS_ALIAS'"
[ -d "$FIXTURE" ] || fail "no iOS fixture at $FIXTURE (bash scripts/dev/build-fixture-app.sh)"

if [ "$(simulator_state "$UDID")" != Booted ]; then
  "$SMIX" sim boot "$UDID" >"$WORK/boot.log" 2>&1 || fail "boot: $(tail -3 "$WORK/boot.log")"
  WE_BOOTED=1
fi
"$SMIX" sim install "$UDID" "$FIXTURE" >"$WORK/install.log" 2>&1 || fail "install: $(tail -3 "$WORK/install.log")"
SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX" runner up "$UDID" --bundle "$APPID" --runner-port "$IOS_PORT" \
  --runner-project "$PROJECT" >"$WORK/up.log" 2>&1 || fail "runner up: $(tail -5 "$WORK/up.log")"
WE_UPPED=1

RUNNER_LOG="$ROOT/.smix/runner/runner-$UDID.log"
[ -f "$RUNNER_LOG" ] || cannot_judge "no runner log at $RUNNER_LOG to count requests in"
fills() { grep -c 'request: POST /fill' "$RUNNER_LOG" || true; }
tree() { curl -s -m 60 -H "App-Bundle-Id: $APPID" "http://127.0.0.1:$IOS_PORT/tree"; }
field_value() {
  tree | python3 -c '
import json, sys
def walk(n):
    if n.get("identifier") == "uks_email":
        print(n.get("value") or ""); raise SystemExit
    for c in n.get("children") or []:
        walk(c)
walk(json.load(sys.stdin))
print("-")'
}
run() { # $1 flow, the rest smix run flags; stdout to $WORK/out, stderr to $WORK/err
  local flow="$1"; shift
  SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX_RUN" --device "$UDID" --runner-port "$IOS_PORT" "$@" "$flow" \
    >"$WORK/out" 2>"$WORK/err"
}
open_flow() { # $1 path
  printf 'appId: %s\n---\n- launchApp: { clearState: true, arguments: ["-uikit-signin"] }\n' "$APPID" >"$1"
  printf -- '- extendedWaitUntil: { visible: { id: uks_email }, timeout: 15000 }\n' >>"$1"
}

# ---- 1. a fill is one request ----
open_flow "$WORK/fill.yaml"
printf -- '- inputText: { id: uks_email, text: "%s" }\n' "$TEXT" >>"$WORK/fill.yaml"
before="$(fills)"
run "$WORK/fill.yaml" || { tail -10 "$WORK/err" >&2; fail "the fill flow failed"; }
sent=$(( $(fills) - before ))
held="$(field_value)"
log "fill of ${#TEXT} characters: $sent POST /fill, the field holds \"$held\""
[ "$held" = "$TEXT" ] || fail "the field holds \"$held\", not \"$TEXT\""
[ "$sent" = 1 ] || fail "$sent requests for one fill"

# ---- 2. settle as configured ----
settle_ms() { # the settle phase of the last tapOn, from --format json
  python3 - "$WORK/out" <<'PY'
import json, sys
out = json.loads(open(sys.argv[1]).read().strip().splitlines()[-1])
taps = [s for s in out["timing"]["steps"] if s["summary"].startswith("tapOn")]
ph = {p["name"]: p["ms"] for p in taps[-1]["phases"]}
print(ph.get("settle", -1))
PY
}
open_flow "$WORK/tap.yaml"
printf -- '- tapOn: { id: uks_continue }\n' >>"$WORK/tap.yaml"
run "$WORK/tap.yaml" --format json || { tail -10 "$WORK/err" >&2; fail "the tap flow failed"; }
default_settle="$(settle_ms)"
run "$WORK/tap.yaml" --format json --settle-frames 1 || { tail -10 "$WORK/err" >&2; fail "the tap flow failed with --settle-frames 1"; }
one_settle="$(settle_ms)"
log "tap settle: default ${default_settle} ms, --settle-frames 1 ${one_settle} ms"
[ "$default_settle" -ge 90 ] || cannot_judge "the default settle took ${default_settle} ms, under one reading's 100 ms wait"
[ "$one_settle" -ge 0 ] && [ "$one_settle" -lt 30 ] || fail "--settle-frames 1 still settled for ${one_settle} ms"

# ---- 3. give up on a still screen ----
open_flow "$WORK/never.yaml"
printf -- '- extendedWaitUntil: { visible: { id: never-there }, timeout: 30000 }\n' >>"$WORK/never.yaml"
began=$(date +%s)
rc=0; run "$WORK/never.yaml" --give-up-when-still-ms 1500 || rc=$?
took=$(( $(date +%s) - began ))
log "wait for an element that never comes, giving up when still: exit $rc after ${took} s"
[ "$rc" != 0 ] || fail "a wait for an element that is not there passed"
grep -q "had not changed for 1500 ms" "$WORK/err" "$WORK/out" || { tail -8 "$WORK/err" >&2; fail "the failure does not say the screen stood still"; }
[ "$took" -lt 20 ] || fail "it took ${took} s; the 30 s timeout ran out instead"

# ---- 4. three flows in one run against three runs ----
open_flow "$WORK/seg-a.yaml"
for seg in b c; do
  printf 'appId: %s\n---\n- tapOn: { id: uks_email }\n- inputText: "%s"\n' "$APPID" "$seg" >"$WORK/seg-$seg.yaml"
done
ms() { python3 -c 'import time; print(int(time.time()*1000))'; }
t0=$(ms)
for seg in a b c; do run "$WORK/seg-$seg.yaml" || { tail -8 "$WORK/err" >&2; fail "segment $seg alone failed"; }; done
t1=$(ms)
SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX_RUN" --device "$UDID" --runner-port "$IOS_PORT" \
  "$WORK/seg-a.yaml" "$WORK/seg-b.yaml" "$WORK/seg-c.yaml" >"$WORK/out" 2>"$WORK/err" \
  || { tail -8 "$WORK/err" >&2; fail "the three segments in one run failed"; }
t2=$(ms)
alone=$(( t1 - t0 )) together=$(( t2 - t1 ))
log "three segments: one by one ${alone} ms, in one run ${together} ms"
[ "$together" -lt "$alone" ] || fail "one run of three took ${together} ms, not less than ${alone} ms one by one"

log "V19.0-C2-E2E-PASS"
