#!/usr/bin/env bash
# v3.0-C1c: `App-Bundle-Id` reaches the routes that drive the app.
#
# A request says which app it means with the `App-Bundle-Id` header, and
# `contextGuardedResponse` is the only thing that reads it. `POST /find`
# never used the wrapper, so `currentContext` stayed at its default and
# every find ran against whichever app the runner booted with. `/fill`
# and `/clear` were missing it too — worse than a failed lookup, since
# they typed into the wrong app and reported success.
#
# It looked like a matching bug for as long as nobody measured it. What
# settled it was `rebound` in the find diagnostics: with the header and
# without it, the runner reported the identical `candidates` count and
# `rebound:false`. The rebinding was not weak, it never happened.
#
# So this gate asserts BOTH directions, and the second one is the point:
#   - with the header, an element of the OTHER app is found
#   - without it, the same request does NOT find that element
# A one-directional check passes whenever the runner happens to be bound
# to the fixture already, which is the state this defect hides in.
#
# `curl` rather than a flow: the header is a wire-level contract, and a
# flow would exercise the client's construction of it as well. When this
# breaks, the question is whether the runner read the header, and this
# asks the runner directly.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
UDID="${SMIX_CROSSAPP_E2E_UDID:-}"
# A port of this gate's own, so a bystander runner cannot turn it red.
. "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
# The runner boots on Preferences and the fixture is the other app, so
# the header is the only thing that can bridge them.
BOOT_BUNDLE="com.apple.Preferences"
FIXTURE_BUNDLE="$(grep -m1 '^BUNDLE_ID=' "$ROOT/scripts/dev/build-fixture-app.sh" | cut -d'"' -f2)"
# Text belonging to the fixture and to nothing else, read out of the
# fixture rather than copied here — a copy goes stale on a rename, and
# this gate would then report the defect it exists to catch. The first
# draft did hardcode an id, with this same comment above it promising
# otherwise; the id it named was the Android fixture's.
#
# Text and not an id, because `/find` is the text-only fast path: it
# decodes `{"selector":{"text":…}}` and refuses anything else with
# `missingText`, and the SDK sends id selectors to `/tree` instead. The
# header question is the same for both routes, and this is the one whose
# wrapper was missing.
FIXTURE_SRC="$ROOT/test-fixtures/demo-app/main.swift"
FIXTURE_TEXT="$(grep -m1 -oE 'Text\("[^"]+"\)' "$FIXTURE_SRC" 2>/dev/null \
  | sed 's/Text("//; s/")//')"
WORK="$(mktemp -d)"

log()  { printf '[c1c] %s\n' "$*" >&2; }
fail() { printf '[c1c] FAIL: %s\n' "$*" >&2; exit 1; }
skip() { printf '[c1c] SKIP: %s\n' "$*" >&2; exit 0; }

started_runner=0
cleanup() {
  if [ "$started_runner" = 1 ]; then
    "$SMIX" runner down >/dev/null 2>&1 \
      || log "warning: runner down did not exit clean"
  fi
  rm -rf "$WORK"
}
trap cleanup EXIT

[ -x "$SMIX" ] || skip "no smix binary at $SMIX (build it, or set SMIX_BIN)"
if [ -z "$UDID" ]; then
  skip "set SMIX_CROSSAPP_E2E_UDID to a booted simulator"
fi

# The text has to have come out of the fixture, or a green run means it
# was absent from both apps rather than present in one — the same answer
# for opposite reasons.
[ -n "$FIXTURE_TEXT" ] \
  || fail "no Text(\"…\") literal found in $FIXTURE_SRC — without one,
       'not found without the header' would be true for the wrong reason
       and this gate would report a pass it did not earn"
log "asserting on \"$FIXTURE_TEXT\", read from the fixture source"

log "building and installing the fixture app"
bash "$ROOT/scripts/dev/build-fixture-app.sh" >"$WORK/build.log" 2>&1 \
  || { tail -20 "$WORK/build.log" >&2; fail "the fixture app did not build"; }
"$SMIX" sim install "$UDID" "$ROOT/test-fixtures/demo-app/build/SmixFixture.app" \
  >"$WORK/install.log" 2>&1 \
  || { tail -20 "$WORK/install.log" >&2; fail "the fixture app did not install"; }

log "runner up on $BOOT_BUNDLE (port $PORT) — deliberately NOT the fixture"
"$SMIX" runner up "$UDID" --bundle "$BOOT_BUNDLE" >"$WORK/up.log" 2>&1 \
  || { tail -25 "$WORK/up.log" >&2; fail "runner up failed"; }
started_runner=1

log "bringing the fixture to the front"
xcrun simctl launch "$UDID" "$FIXTURE_BUNDLE" >/dev/null 2>&1 \
  || fail "could not launch the fixture app"
# The runner reads what is on screen; give the launch a moment to settle
# rather than racing it. A retry loop would hide a slow launch as a
# flake, and this gate's whole subject is a wrong answer that looks calm.
sleep 3

find_with_header() {
  curl -s -m 20 -X POST -H 'content-type: application/json' \
    -H "App-Bundle-Id: $1" \
    -d "{\"selector\":{\"text\":\"$FIXTURE_TEXT\"}}" \
    "http://127.0.0.1:$PORT/find"
}
find_without_header() {
  curl -s -m 20 -X POST -H 'content-type: application/json' \
    -d "{\"selector\":{\"text\":\"$FIXTURE_TEXT\"}}" \
    "http://127.0.0.1:$PORT/find"
}

log "with the header: expect the fixture's text to be found"
with="$(find_with_header "$FIXTURE_BUNDLE")"
printf '%s\n' "$with" >"$WORK/with-header.json"
case "$with" in
  *'"found":true'*) log "found — the header rebound the runner to $FIXTURE_BUNDLE" ;;
  *) fail "the runner did not find \"$FIXTURE_TEXT\" in $FIXTURE_BUNDLE with the
       header set. The response was:
       $with" ;;
esac

log "without the header: expect NOT found (the runner is still on $BOOT_BUNDLE)"
without="$(find_without_header)"
printf '%s\n' "$without" >"$WORK/without-header.json"
case "$without" in
  *'"found":false'*)
    log "not found — which is what makes the line above mean something" ;;
  *'"found":true'*)
    fail "the fixture's text was found WITHOUT the header, so the run above
       proves nothing: the runner is bound to the fixture regardless. Either
       the boot bundle is wrong, or something rebound it outside this gate." ;;
  *) fail "unreadable response without the header:
       $without" ;;
esac

log "PASS — App-Bundle-Id decides which app /find reads"
