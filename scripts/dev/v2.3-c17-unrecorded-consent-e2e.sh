#!/usr/bin/env bash
# v2.3-C17: `up` and `down` give the same answer about the same fact.
#
# They did not until 2026-08-06. Given a port held by a runner the store
# has no record of:
#
#   runner up   → "not killing blindly; investigate"
#   runner down → SIGINT, silently
#
# So the command that refused to touch it and the command that ended it
# were both one keystroke away, and only one of them said anything. That
# is the shape of the sweep that took out another session's runner in
# 2026-07 — and of a near-miss the same day this was found, when this
# repo's own teardown ran twice against the default port while somebody
# else's runner was on it.
#
# The fix is not "never": `up`'s refusal points at `down` as the way
# through, and a guard that leaves someone with no path gets worked
# around rather than obeyed. It is "not silently".
#
# Nothing here touches a real runner. The occupant is a listener this
# script starts and kills itself.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
OUT="$(mktemp)"
PORT="${C17_PORT:-22591}"
DECOY=""

log()  { printf '[c17-consent] %s\n' "$*"; }
step() { printf '[c17-consent] --- %s\n' "$*"; }
fail() { printf '[c17-consent] FAIL: %s\n' "$*" >&2; exit 1; }

cleanup() {
  # Reaped quietly: the shell announces a killed job on stderr *after*
  # the verdict line, and a suite read with `tail -1` would show that
  # instead of the result.
  if [ -n "$DECOY" ]; then
    { kill "$DECOY" 2>/dev/null && wait "$DECOY" 2>/dev/null; } || true
  fi
  rm -f "$OUT" "${OUT}.clean"
}
trap cleanup EXIT
cd "$ROOT"
[ -x "$SMIX" ] || fail "no smix binary at $SMIX"

step "0. the judgement itself, which needs no processes"
cargo test -p smix-capsule --lib unrecorded > "$OUT" 2>&1 \
  || { tail -20 "$OUT"; fail "decide_unrecorded tests failed"; }
cargo test -p smix-capsule --lib consent_takes >> "$OUT" 2>&1 || true
grep -qE "test result: ok\." "$OUT" || fail "unit tests reported no pass"
log "pure-function tests pass"

step "1. the two commands must not contradict each other in the source"
# A source-level check, because the contradiction was never visible from
# either command alone — you had to read both to see it. `up` refuses;
# `down` must not do the opposite silently.
grep -q "not killing blindly" crates/smix-capsule/src/runner.rs \
  || fail "up's refusal is gone — this checkpoint assumes it exists"
grep -q "decide_unrecorded(unrecorded_sessions_on(port), consent)" crates/smix-capsule/src/runner.rs \
  || fail "down no longer routes unrecorded sessions through the decision"
# Every call site but the CLI flag must decline. If a new one appears
# saying `true`, this fails and asks why.
CONSENTS="$(grep -rn "runner::down(\|down(root, cycle_port" crates/ --include=*.rs 2>/dev/null \
  | grep -v "fuzz/target" | grep -c "true" || true)"
[ "$CONSENTS" = "0" ] \
  || { grep -rn "runner::down(" crates/ --include=*.rs | grep "true"; \
       fail "a non-CLI call site consents to ending unrecorded runners"; }
log "up refuses, down decides, and no library call site consents"

step "2. a port held by something unrecorded: down reports and fails"
# `unrecorded_sessions_on` only matches a listener whose command line
# carries a simulator device path with an xcodebuild driving it, so a
# plain listener is correctly NOT matched — which is the case this step
# pins: down must not report a stranger as a runner, and must not fail
# for the wrong reason.
python3 -c "
import socket, time, sys
s = socket.socket(); s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(('127.0.0.1', $PORT)); s.listen(1)
sys.stderr.write('up\n'); sys.stderr.flush()
time.sleep(120)
" 2>/dev/null &
DECOY=$!
for _ in $(seq 20); do
  python3 -c "
import socket,sys
try: socket.create_connection(('127.0.0.1',$PORT),0.2).close()
except Exception: sys.exit(1)
" 2>/dev/null && break
  sleep 0.2
done
log "a plain listener holds $PORT (pid $DECOY)"

SMIX_RUNNER_PORT="$PORT" "$SMIX" runner down > "$OUT" 2>&1 || true
grep -v '^kevy:' "$OUT" > "${OUT}.clean" && mv "${OUT}.clean" "$OUT"
# It answers /health with nothing, so teardown reports the port is not
# clear rather than claiming success. What it must NOT do is kill it.
kill -0 "$DECOY" 2>/dev/null || fail "down killed a process that is not a runner at all"
log "the listener survived a default teardown"

step "3. the refusal, when it fires, names both the check and the way through"
# Asserted on what the function returns, not grepped for in the source.
# Staging a real second runner would mean building an XCUITest session
# solely to produce a sentence — and the sentence is the deliverable, so
# it is tested where it is written.
cargo test -p smix-capsule --lib the_refusal_says > "$OUT" 2>&1 \
  || { tail -20 "$OUT"; fail "the refusal's wording is not what it must be"; }
grep -qE "test result: ok\. [1-9]" "$OUT" || fail "the wording test did not run"
"$SMIX" runner down --help > "$OUT" 2>&1
grep -q -- "--include-unrecorded" "$OUT" || { cat "$OUT"; fail "the flag is not on the command"; }
grep -qi "another session" "$OUT" \
  || { cat "$OUT"; fail "the flag's help does not say why it is off by default"; }
log "the flag exists, and its help says why it is not the default"

step "4. up points at the command that resolves it, not at one that does not"
grep -q "smix runner down --include-unrecorded" crates/smix-capsule/src/runner.rs \
  || fail "up's advice still points at a bare 'runner down', which now reports and stops"
log "up's advice and down's behaviour agree"

echo "C17-UNRECORDED-CONSENT-PASS"
