#!/usr/bin/env bash
# v2.12-C3 federation single-node e2e: the scheduler machine drives one
# remote node (mini by default) end to end — source sync -> config
# authority sync -> rebuild + freshness stamp -> readiness gate -> sim
# prep (explicit UDID, §9#1 sims only) -> the ignored Rust e2e test
# (parse_nodes → expand_slots → assign_flows → gate → remote_argv →
# run_ssh → JSON report lines) -> teardown. Every stage is machine
# judged; any failure stops the script with a FAIL marker.
set -euo pipefail

HOST="${SMIX_FED_NODE_HOST:-mini}"
REPO="${SMIX_FED_NODE_REPO:-workspace/goliajp/smix}"   # remote, relative to $HOME
SIM_NAME="sim-simx-001"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
FLOW_A="scripts/release/stress-corpus/launch-and-capture.yaml"
FLOW_B="scripts/release/stress-corpus/screenshot-twice.yaml"

log()  { printf '[c3-fed] %s\n' "$*"; }
fail() { printf '[c3-fed] FAIL: %s\n' "$*" >&2; exit 1; }

# An unmet precondition is not this suite's failure to report. Yielding to
# somebody else's batch, or a node that is not reachable here, says nothing
# about whether smix works — and FAIL says it does not, to whoever reads
# the suite next. A suite that cries wolf gets skimmed, and then a real
# failure gets skimmed with it.
skip() { printf '[c3-fed] %s\n' "$*" >&2; printf '%s\n' "C3-FEDERATION-SINGLE-SKIP"; exit 0; }


rssh() { ssh -o ConnectTimeout=5 -o BatchMode=yes "$HOST" "$@"; }

# --- 1. guards: node reachable, no batch owner on either side, flows exist ---
log "guard: $HOST reachable"
rssh true || skip "$HOST is not reachable over BatchMode ssh — this node is not available here"
REMOTE_REPO="$(rssh "cd $REPO && pwd")" || fail "remote repo $REPO missing on $HOST"
log "guard: no active batch on studio or $HOST (yield, never seize)"
pgrep -f 'runner.ts|smix run|supervise' >/dev/null && skip "batch owner active on studio — yielding; re-run when it is idle"
rssh "pgrep -f 'runner.ts|smix run|supervise' >/dev/null" && skip "batch owner active on $HOST — yielding; re-run when it is idle"
log "guard: no user build in flight on $HOST"
rssh "pgrep -f 'cargo build|xcodebuild' >/dev/null" && skip "user build in flight on $HOST — yielding; re-run when it is idle"
[ -f "$ROOT/$FLOW_A" ] || fail "corpus flow missing: $FLOW_A"
[ -f "$ROOT/$FLOW_B" ] || fail "corpus flow missing: $FLOW_B"

WORK="$(mktemp -d)"
UDID=""
cleanup() {
  log "teardown: runner down + sim shutdown + remote artifacts + workdir"
  if [ -n "$UDID" ]; then
    rssh "cd '$REMOTE_REPO' && target/release/smix runner down --device $UDID" || true
    rssh "cd '$REMOTE_REPO' && target/release/smix sim shutdown $UDID" || true
  fi
  rssh "cd '$REMOTE_REPO' && rm -f launch-capture.png shot-1.png shot-2.png" || true
  rm -rf "$WORK"
}
trap cleanup EXIT

# --- 2. source sync (house rsync shape; exclusion set not shrunk) ---
log "rsync sources -> $HOST:$REPO/"
rsync -a --stats --exclude='target/' --exclude='.git/' --exclude='node_modules/' \
  --exclude='.smix/' --exclude='swift-bridge/.build/' --exclude='*/build/' \
  --exclude='.scratch/' "$ROOT/" "$HOST:$REPO/"

# --- 3. config authority sync: remote mirrors studio's .smix/config.yaml ---
if [ -f "$ROOT/.smix/config.yaml" ]; then
  log "config sync: studio config.yaml -> $HOST"
  rsync -a "$ROOT/.smix/config.yaml" "$HOST:$REPO/.smix/config.yaml"
  rssh "test -f '$REMOTE_REPO/.smix/config.yaml'" || fail "config sync: remote config.yaml missing after sync"
else
  log "config sync: studio has no config.yaml — ensuring $HOST has none"
  rssh "rm -f '$REMOTE_REPO/.smix/config.yaml'"
  rssh "test ! -f '$REMOTE_REPO/.smix/config.yaml'" || fail "config sync: remote config.yaml still present"
fi

# --- 4. rebuild + freshness stamp (cargo judges freshness; stamp only on success) ---
log "remote rebuild (cargo build --release -p smix-cli) + stamp"
rssh "cd '$REMOTE_REPO' && cargo build --release -p smix-cli && touch target/.smix-fed-stamp" \
  || fail "remote rebuild failed — not running stale"

# --- 5. readiness gate, independent recheck (same shape as readiness_argv) ---
log "readiness gate recheck"
rssh "cd '$REMOTE_REPO' && test -f target/.smix-fed-stamp && test -x target/release/smix && [ -z \"\$(find crates -name '*.rs' -newer target/.smix-fed-stamp)\" ]" \
  || fail "readiness gate red after rebuild — sync/rebuild sequence did not converge"

# --- 6. device resolution + prep (§9#1 sims only, explicit UDID) ---
log "resolve $SIM_NAME UDID on $HOST"
SIM_LINES="$(rssh "cd '$REMOTE_REPO' && target/release/smix sim list 2>/dev/null" | grep -F "$SIM_NAME")" \
  || fail "$SIM_NAME not in remote sim list"
[ "$(printf '%s\n' "$SIM_LINES" | wc -l | tr -d ' ')" = 1 ] \
  || fail "$SIM_NAME matches more than one sim list line: $SIM_LINES"
UDID="$(printf '%s\n' "$SIM_LINES" | grep -oE '[0-9A-F]{8}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{12}')" \
  || fail "no UDID in sim list line: $SIM_LINES"
log "sim boot + runner up ($UDID)"
rssh "cd '$REMOTE_REPO' && target/release/smix sim boot $UDID" || true
rssh "cd '$REMOTE_REPO' && target/release/smix runner up $UDID --bundle com.apple.Preferences" \
  || fail "runner up did not reach ready on $HOST"

# --- 7. drive the ignored Rust e2e test (the product-side leg) ---
log "drive federation_e2e_single_node_runs_flows_on_mini (ignored test)"
cat >"$WORK/nodes.yaml" <<YAML
nodes:
  - name: c3-node
    host: $HOST
    repo: $REMOTE_REPO
    devices: [$UDID]
YAML
(
  cd "$ROOT"
  SMIX_FED_E2E_NODES="$WORK/nodes.yaml" \
  SMIX_FED_E2E_FLOWS="$FLOW_A,$FLOW_B" \
    cargo test -p smix-cli --bin smix federation_e2e -- --ignored --nocapture
) || fail "ignored e2e test red — the single-node loop did not close"

# --- 8. marker (teardown runs via trap) ---
log "C3-FED-E2E-PASS"
