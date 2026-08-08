#!/usr/bin/env bash
# v2.14-C1: `fill` replaces a named field, on a real simulator.
#
# The guides have said "Fill (replaces focused field content)" since the
# verb existed. The runner typed on the end of whatever was there, so
# filling a field twice left both values concatenated. In a password
# field that is invisible — the dots look right — and it surfaces as a
# login rejecting a correct password, which is what it cost the
# consumer who reported it.
#
# Unit tests pin the wire (`clearFirst` rides the first chunk alone),
# but the thing that was wrong was what appears in the field, and only
# a device can say that. The fixture app is the instrument: its result
# label stays empty until Submit is pressed, so asserting on it
# distinguishes "the field holds this" from "I typed this" — a `/tree`
# read of the field's own value would be measuring the same runner path
# under test.
#
# The two halves of the rule are both checked here, because the rule is
# what changed and half of it is the part nobody would think to verify:
#   - a named field is replaced
#   - typing into whatever holds focus still appends
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
UDID="${SMIX_FILL_E2E_UDID:-}"
PORT="${SMIX_FILL_E2E_PORT:-22101}"
# Read from the build script rather than repeated here: a bundle id
# that drifts makes the runner fail to attach, and the failure it
# produces ("has not loaded accessibility") does not name the cause.
BUNDLE="$(grep -m1 '^BUNDLE_ID=' "$ROOT/scripts/dev/build-fixture-app.sh" | cut -d'"' -f2)"
WORK="$(mktemp -d)"

log()  { printf '[c1-fill] %s\n' "$*" >&2; }
step() { printf '[c1-fill] --- %s\n' "$*" >&2; }
fail() { printf '[c1-fill] FAIL: %s\n' "$*" >&2; exit 1; }
skip() { printf '[c1-fill] SKIP: %s\n' "$*" >&2; exit 0; }

started_runner=0
cleanup() {
  if [ "$started_runner" = 1 ]; then
    # Not `|| true` on its own: a teardown that cannot fail is a
    # teardown nobody notices failing, and this one did — the first
    # version passed `--runner-port` to a `down` that had no such flag,
    # the parse failed, and the runner outlived every green run.
    if ! "$SMIX" runner down --runner-port "$PORT" >"$WORK/down.log" 2>&1; then
      printf '[c1-fill] the runner on %s was not stopped:\n' "$PORT" >&2
      tail -3 "$WORK/down.log" >&2
    fi
    if pgrep -f "id=$UDID" >/dev/null 2>&1; then
      printf '[c1-fill] a runner process for %s is still alive\n' "$UDID" >&2
    fi
  fi
  rm -rf "$WORK"
}
trap cleanup EXIT

smix() { "$SMIX" "$@" 2>&1 | grep -v '^kevy:' || true; }

[ -x "$SMIX" ] || fail "no smix binary at $SMIX (cargo build -p smix-cli)"
command -v xcrun >/dev/null 2>&1 || skip "no xcrun — this needs a simulator; run it on a Mac with Xcode"

if [ -z "$UDID" ]; then
  # A simulator this repository owns, never whichever one is booted:
  # somebody else's device is somebody else's work.
  UDID="$(xcrun simctl list devices -j 2>/dev/null \
    | python3 -c '
import json,sys
for rt, ds in json.load(sys.stdin)["devices"].items():
    for d in ds:
        if d["name"].startswith("sim-smix-") and d.get("isAvailable"):
            print(d["udid"]); raise SystemExit
' || true)"
fi
[ -n "$UDID" ] || skip "no sim-smix-* simulator on this machine — create one, or set SMIX_FILL_E2E_UDID"
log "device $UDID, runner port $PORT"

step "0. the wire rule, which needs no device"
( cd "$ROOT" && cargo test -p smix-driver --test driver clear_first_belongs ) > "$WORK/unit.log" 2>&1 \
  || { tail -20 "$WORK/unit.log"; fail "the chunking rule's test failed"; }
grep -qE "test result: ok\. [1-9]" "$WORK/unit.log" || fail "the chunking test did not run"
log "clearFirst rides the first chunk alone"

step "1. build and install the fixture app"
bash "$ROOT/scripts/dev/build-fixture-app.sh" > "$WORK/build.log" 2>&1 \
  || { tail -20 "$WORK/build.log"; fail "the fixture app did not build"; }
APP="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
[ -d "$APP" ] || fail "no fixture app at $APP after a successful build"
smix sim boot "$UDID" >/dev/null || fail "could not boot $UDID"
smix sim install "$UDID" "$APP" >/dev/null || fail "could not install the fixture"

step "2. runner up"
smix runner up "$UDID" --bundle "$BUNDLE" --runner-port "$PORT" > "$WORK/up.log" 2>&1 \
  || { tail -20 "$WORK/up.log"; fail "runner did not come up"; }
started_runner=1
log "runner answering on $PORT"

step "3. fill a named field twice"
smix fill "id:fixture-input" --text "first" --port "$PORT" >/dev/null \
  || fail "the first fill failed"
smix fill "id:fixture-input" --text "second" --port "$PORT" >/dev/null \
  || fail "the second fill failed"
smix tap "id:fixture-submit" --port "$PORT" >/dev/null || fail "submit did not tap"

# Read the label, not the field: the label is written by the app from
# what it actually received.
# Not the `smix` helper: it folds stderr into stdout, and one stray
# diagnostic line would make this JSON unparseable in a way that reads
# like a broken tree.
"$SMIX" tree --port "$PORT" --json 2>"$WORK/tree.err" | grep -v '^kevy:' > "$WORK/tree.json" || true
head -c 1 "$WORK/tree.json" | grep -q '{' \
  || { head -5 "$WORK/tree.err"; fail "the runner did not return a tree — see above"; }
RESULT="$(python3 -c '
import json,sys
def walk(n):
    if n.get("identifier") == "fixture-result":
        print(n.get("label") or n.get("value") or n.get("text") or "")
        raise SystemExit
    for c in n.get("children", []): walk(c)
walk(json.load(open(sys.argv[1])))
' "$WORK/tree.json")"
log "submitted value: ${RESULT:-<empty>}"
[ "$RESULT" = "second" ] || fail "a named field must be replaced — the app received ${RESULT:-<empty>}, wanted 'second'"
log "replaced, not concatenated"

step "4. typing into the focused field still appends"
# The scalar `inputText:` verb, which is maestro's shape: no field is
# named, so there is nothing to replace, and a flow that types twice
# means the second to continue the first.
cat > "$WORK/append.yaml" <<YAML
appId: $BUNDLE
---
- tapOn:
    id: "fixture-input"
- inputText: "ab"
- inputText: "cd"
- tapOn:
    id: "fixture-submit"
- assertVisible:
    id: "fixture-result"
    text: "abcd"
YAML
smix run --device "$UDID" --runner-port "$PORT" --no-launch "$WORK/append.yaml" > "$WORK/append.log" 2>&1 \
  || { tail -25 "$WORK/append.log"; fail "typing into the focused field must append — it did not produce 'abcd'"; }
log "appended, because nothing was named"

printf 'v2.14-C1 FILL-E2E-PASS\n'
