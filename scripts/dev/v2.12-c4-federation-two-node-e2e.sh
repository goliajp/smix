#!/usr/bin/env bash
# v2.12-C4 federation two-node e2e: the scheduler machine drives TWO
# live nodes — itself over `ssh localhost` (dedicated runner port, so it
# coexists with any resident runner on the default port) and mini —
# end to end: localhost self-authorization -> source sync (mini) ->
# config authority sync (mini) -> rebuild + freshness stamp (both) ->
# readiness gate (both) -> sim prep (explicit UDID, §9#1 sims only) ->
# the ignored Rust e2e test (parse_nodes → expand_slots → assign_flows
# → per-node gate → remote run with --debug-output passthrough → JSON
# report lines → merge_reports → per-node artifact rsync recovery) ->
# teardown. Every stage is machine judged; any failure stops the script
# with a FAIL marker.
set -euo pipefail

HOST="${SMIX_FED_NODE_HOST:-mini}"
STUDIO_PORT="${SMIX_FED_STUDIO_PORT:-22097}"
STUDIO_SIM="${SMIX_FED_STUDIO_SIM:-sim-smix-02}"
MINI_SIM="${SMIX_FED_MINI_SIM:-sim-simx-001}"
REPO="workspace/goliajp/smix"   # remote, relative to $HOME
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
FLOW_A="scripts/release/stress-corpus/launch-and-capture.yaml"
FLOW_B="scripts/release/stress-corpus/screenshot-twice.yaml"
ARTIFACT_DIR=".smix/fed-artifacts"

log()  { printf '[c4-fed] %s\n' "$*"; }
fail() { printf '[c4-fed] FAIL: %s\n' "$*" >&2; exit 1; }

# An unmet precondition is not this suite's failure to report. Yielding to
# somebody else's batch, or a node that is not reachable here, says nothing
# about whether smix works — and FAIL says it does not, to whoever reads
# the suite next. A suite that cries wolf gets skimmed, and then a real
# failure gets skimmed with it.
skip() { printf '[c4-fed] %s\n' "$*" >&2; printf '%s\n' "C4-FEDERATION-TWO-NODE-SKIP"; exit 0; }


rssh() { ssh -o ConnectTimeout=5 -o BatchMode=yes "$HOST" "$@"; }
lssh() { ssh -o ConnectTimeout=5 -o BatchMode=yes localhost "$@"; }

# --- 1. guards ---
log "guard: localhost self-authorization (idempotent)"
[ -f "$HOME/.ssh/id_ed25519.pub" ] || fail "no ~/.ssh/id_ed25519.pub to self-authorize with"
if ! grep -qF "$(awk '{print $2}' "$HOME/.ssh/id_ed25519.pub")" "$HOME/.ssh/authorized_keys"; then
  log "guard: appending own public key to ~/.ssh/authorized_keys"
  cat "$HOME/.ssh/id_ed25519.pub" >> "$HOME/.ssh/authorized_keys"
fi
ssh -o ConnectTimeout=5 -o BatchMode=yes -o StrictHostKeyChecking=accept-new localhost true \
  || fail "ssh localhost refused after self-authorization"

log "guard: $HOST reachable"
rssh true || skip "$HOST is not reachable over BatchMode ssh — this node is not available here"
REMOTE_REPO="$(rssh "cd $REPO && pwd")" || fail "remote repo $REPO missing on $HOST"

log "guard: no active batch on studio or $HOST (yield, never seize)"
pgrep -f 'runner.ts|smix run|supervise' >/dev/null && skip "batch owner active on studio — yielding; re-run when it is idle"
rssh "pgrep -f 'runner.ts|smix run|supervise' >/dev/null" && skip "batch owner active on $HOST — yielding; re-run when it is idle"

log "guard: no user build in flight ($HOST: cargo/xcodebuild; studio: cargo only — resident runner capsule is legitimate)"
rssh "pgrep -f 'cargo build|xcodebuild' >/dev/null" && skip "user build in flight on $HOST — yielding; re-run when it is idle"
# `skip`, not `fail` — the word "yielding" is right there. A script
# that detects a condition it will not disturb and reports failure makes
# a gate red for something the product did not do, and a gate that goes
# red for reasons unrelated to the code is one people stop reading. The
# line above this one already treats a busy batch owner as a skip.
pgrep -f 'cargo build' >/dev/null && skip "cargo build in flight on studio — yielding"

log "guard: studio runner port $STUDIO_PORT free"
lsof -nP -i ":$STUDIO_PORT" >/dev/null 2>&1 && skip "port $STUDIO_PORT busy on studio — yielding"

[ -f "$ROOT/$FLOW_A" ] || fail "corpus flow missing: $FLOW_A"
[ -f "$ROOT/$FLOW_B" ] || fail "corpus flow missing: $FLOW_B"

WORK="$(mktemp -d)"
mkdir -p "$WORK/pull"
UDID_S=""
UDID_M=""
cleanup() {
  log "teardown: runners down + sims shutdown + artifacts + workdir"
  if [ -n "$UDID_M" ]; then
    rssh "cd '$REMOTE_REPO' && target/release/smix runner down" || true
    rssh "cd '$REMOTE_REPO' && target/release/smix sim shutdown $UDID_M" || true
  fi
  rssh "cd '$REMOTE_REPO' && rm -rf $ARTIFACT_DIR && rm -f launch-capture.png shot-1.png shot-2.png" || true
  if [ -n "$UDID_S" ]; then
    # iOS `runner down` selects its port from SMIX_RUNNER_PORT only —
    # never run it bare here, that would hit the default port's runner.
    ( cd "$ROOT" && SMIX_RUNNER_PORT="$STUDIO_PORT" target/release/smix runner down ) || true
    ( cd "$ROOT" && target/release/smix sim shutdown "$UDID_S" ) || true
  fi
  rm -rf "$ROOT/$ARTIFACT_DIR"
  rm -f "$ROOT/launch-capture.png" "$ROOT/shot-1.png" "$ROOT/shot-2.png"
  rm -rf "$WORK"
}
trap cleanup EXIT

# --- 2. source sync, mini only (house rsync shape; exclusion set not shrunk).
#     The studio node's repo IS this checkout — the authority copies to no one. ---
log "rsync sources -> $HOST:$REPO/"
rsync -a --stats --exclude='target/' --exclude='.git/' --exclude='node_modules/' \
  --exclude='.smix/' --exclude='swift-bridge/.build/' --exclude='*/build/' \
  --exclude='.scratch/' "$ROOT/" "$HOST:$REPO/"

# --- 3. config authority sync, mini only: remote mirrors studio's .smix/config.yaml ---
if [ -f "$ROOT/.smix/config.yaml" ]; then
  log "config sync: studio config.yaml -> $HOST"
  rsync -a "$ROOT/.smix/config.yaml" "$HOST:$REPO/.smix/config.yaml"
  rssh "test -f '$REMOTE_REPO/.smix/config.yaml'" || fail "config sync: remote config.yaml missing after sync"
else
  log "config sync: studio has no config.yaml — ensuring $HOST has none"
  rssh "rm -f '$REMOTE_REPO/.smix/config.yaml'"
  rssh "test ! -f '$REMOTE_REPO/.smix/config.yaml'" || fail "config sync: remote config.yaml still present"
fi

# --- 4. rebuild + freshness stamp, both nodes (cargo judges freshness) ---
log "remote rebuild (cargo build --release -p smix-cli) + stamp on $HOST"
rssh "cd '$REMOTE_REPO' && cargo build --release -p smix-cli && touch target/.smix-fed-stamp" \
  || fail "remote rebuild failed on $HOST — not running stale"
log "local rebuild (cargo build --release -p smix-cli) + stamp on studio"
( cd "$ROOT" && cargo build --release -p smix-cli && touch target/.smix-fed-stamp ) \
  || fail "local rebuild failed on studio — not running stale"

# --- 5. readiness gate, independent recheck on both nodes (same shape as readiness_argv) ---
log "readiness gate recheck: studio (via ssh localhost)"
lssh "cd '$ROOT' && test -f target/.smix-fed-stamp && test -x target/release/smix && [ -z \"\$(find crates -name '*.rs' -newer target/.smix-fed-stamp)\" ]" \
  || fail "studio readiness gate red after rebuild"
log "readiness gate recheck: $HOST"
rssh "cd '$REMOTE_REPO' && test -f target/.smix-fed-stamp && test -x target/release/smix && [ -z \"\$(find crates -name '*.rs' -newer target/.smix-fed-stamp)\" ]" \
  || fail "$HOST readiness gate red after rebuild"

# --- 6. device resolution + prep, both nodes (§9#1 sims only, explicit UDID) ---
log "resolve $STUDIO_SIM UDID on studio"
SIM_LINES_S="$( (cd "$ROOT" && target/release/smix sim list 2>/dev/null) | grep -F "$STUDIO_SIM")" \
  || fail "$STUDIO_SIM not in studio sim list"
[ "$(printf '%s\n' "$SIM_LINES_S" | wc -l | tr -d ' ')" = 1 ] \
  || fail "$STUDIO_SIM matches more than one sim list line: $SIM_LINES_S"
UDID_S="$(printf '%s\n' "$SIM_LINES_S" | grep -oE '[0-9A-F]{8}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{12}')" \
  || fail "no UDID in studio sim list line: $SIM_LINES_S"
log "studio sim boot + runner up ($UDID_S, port $STUDIO_PORT)"
( cd "$ROOT" && target/release/smix sim boot "$UDID_S" ) || true
( cd "$ROOT" && target/release/smix runner up "$UDID_S" --bundle com.apple.Preferences --runner-port "$STUDIO_PORT" ) \
  || fail "runner up did not reach ready on studio port $STUDIO_PORT"

log "resolve $MINI_SIM UDID on $HOST"
SIM_LINES_M="$(rssh "cd '$REMOTE_REPO' && target/release/smix sim list 2>/dev/null" | grep -F "$MINI_SIM")" \
  || fail "$MINI_SIM not in remote sim list"
[ "$(printf '%s\n' "$SIM_LINES_M" | wc -l | tr -d ' ')" = 1 ] \
  || fail "$MINI_SIM matches more than one sim list line: $SIM_LINES_M"
UDID_M="$(printf '%s\n' "$SIM_LINES_M" | grep -oE '[0-9A-F]{8}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{12}')" \
  || fail "no UDID in remote sim list line: $SIM_LINES_M"
log "$HOST sim boot + runner up ($UDID_M, default port)"
rssh "cd '$REMOTE_REPO' && target/release/smix sim boot $UDID_M" || true
rssh "cd '$REMOTE_REPO' && target/release/smix runner up $UDID_M --bundle com.apple.Preferences" \
  || fail "runner up did not reach ready on $HOST"

# --- 7. drive the ignored Rust e2e test (the product-side leg) ---
log "drive federation_e2e_two_nodes_merge_reports_and_recover_artifacts (ignored test)"
cat >"$WORK/nodes.yaml" <<YAML
nodes:
  - name: c4-studio
    host: localhost
    repo: $ROOT
    devices: [$UDID_S]
  - name: c4-mini
    host: $HOST
    repo: $REMOTE_REPO
    devices: [$UDID_M]
YAML
(
  cd "$ROOT"
  SMIX_FED_E2E_NODES="$WORK/nodes.yaml" \
  SMIX_FED_E2E_FLOWS="$FLOW_A,$FLOW_B" \
  SMIX_FED_E2E_RUNNER_PORTS="c4-studio=$STUDIO_PORT" \
  SMIX_FED_E2E_PULL_DIR="$WORK/pull" \
    cargo test -p smix-cli --bin smix federation_e2e_two_nodes -- --ignored --nocapture
) || fail "ignored e2e test red — the two-node loop did not close"
log "recheck recovered artifacts (per-node subdirs)"
[ -f "$WORK/pull/c4-studio/run-summary.json" ] || fail "recovered artifact missing: pull/c4-studio/run-summary.json"
[ -f "$WORK/pull/c4-mini/run-summary.json" ] || fail "recovered artifact missing: pull/c4-mini/run-summary.json"

# --- 8. marker (teardown runs via trap) ---
log "C4-FED-E2E-PASS"
