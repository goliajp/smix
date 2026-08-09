#!/usr/bin/env bash
# v2.12-C5 federation CLI e2e: one real `smix run --nodes` command
# drives TWO live nodes — studio over `ssh localhost` (dedicated runner
# port) and mini — through the whole product lane: roster parse ->
# local flow check -> per-node readiness gate -> concurrent ssh fan-out
# -> per-node fold -> artifact rsync recovery -> merged JSON on stdout
# -> worst-of-nodes exit. The script owns everything around it:
# self-authorization, source/config sync, rebuild + stamp, device prep,
# and a no-sweep teardown (recorded-handle precise kill on studio —
# never any iOS-form `smix runner down` here; its port-agnostic pkill
# fallback is the unfixed C4 incident, fix pending user decision).
# Every stage is machine judged; any failure stops with a FAIL marker.
set -euo pipefail

HOST="${SMIX_FED_NODE_HOST:-mini}"
STUDIO_SIM="${SMIX_FED_STUDIO_SIM:-sim-smix-02}"
MINI_SIM="${SMIX_FED_MINI_SIM:-sim-simx-001}"
REPO="workspace/goliajp/smix"   # remote, relative to $HOME
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

# A port of this gate's own, so a bystander runner cannot turn it red.
. "$ROOT/scripts/lib/gate-port.sh"
STUDIO_PORT="${SMIX_FED_STUDIO_PORT:-$SMIX_RUNNER_PORT}"
FLOW_A="scripts/release/stress-corpus/launch-and-capture.yaml"
FLOW_B="scripts/release/stress-corpus/screenshot-twice.yaml"
ARTIFACT_DIR=".smix/fed-artifacts"

log()  { printf '[c5-fed] %s\n' "$*"; }
fail() { printf '[c5-fed] FAIL: %s\n' "$*" >&2; exit 1; }

# An unmet precondition is not this suite's failure to report. Yielding to
# somebody else's batch, or a node that is not reachable here, says nothing
# about whether smix works — and FAIL says it does not, to whoever reads
# the suite next. A suite that cries wolf gets skimmed, and then a real
# failure gets skimmed with it.
skip() { printf '[c5-fed] %s\n' "$*" >&2; printf '%s\n' "C5-FEDERATION-CLI-SKIP"; exit 0; }


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
pgrep -f 'cargo build' >/dev/null && fail "cargo build in flight on studio — yielding"

log "guard: studio runner port $STUDIO_PORT free"
lsof -nP -i ":$STUDIO_PORT" >/dev/null 2>&1 && fail "port $STUDIO_PORT busy on studio"

[ -f "$ROOT/$FLOW_A" ] || fail "corpus flow missing: $FLOW_A"
[ -f "$ROOT/$FLOW_B" ] || fail "corpus flow missing: $FLOW_B"

log "guard: SMIX_UDID / SMIX_RUNNER_PORT not exported (clap counts env values as present -> --nodes conflict)"
[ -z "${SMIX_UDID:-}" ] || fail "SMIX_UDID is exported — unset it before a --nodes run"
[ -z "${SMIX_RUNNER_PORT:-}" ] || fail "SMIX_RUNNER_PORT is exported — unset it before a --nodes run"

WORK="$(mktemp -d)"
mkdir -p "$WORK/pull"
UDID_S=""
UDID_M=""

# Studio teardown, no sweep (C4 incident discipline): read the recorded
# runner handle from the store, verify the pid is still xcodebuild
# (pid-reuse guard), then a precise kill -INT with a bounded wait.
# Never any iOS-form `smix runner down` here — env or flag — and never
# a bare `pgrep xcodebuild`. The stale handle stays in the store; the
# product drops it itself on the next `runner down`.
stop_studio_runner() {
  local pid cmd i
  pid="$( (cd "$ROOT" && target/release/smix diagnostic store 2>/dev/null) \
    | python3 -c 'import sys,json; print(json.load(sys.stdin)["one:runner-ios"]["pid"])' 2>/dev/null )" || true
  if [ -z "$pid" ]; then
    log "teardown: no recorded runner handle on studio — nothing to stop"
    return 0
  fi
  cmd="$(ps -p "$pid" -o command= 2>/dev/null)" || true
  case "$cmd" in
    *xcodebuild*)
      log "teardown: kill -INT recorded runner pid $pid (verified xcodebuild)"
      kill -INT "$pid" 2>/dev/null || true
      i=0
      while [ "$i" -lt 30 ] && kill -0 "$pid" 2>/dev/null; do
        sleep 1
        i=$((i + 1))
      done
      if kill -0 "$pid" 2>/dev/null; then
        log "teardown: pid $pid survived ${i}s after INT — kill -9"
        kill -9 "$pid" 2>/dev/null || true
      fi
      ;;
    "")
      log "teardown: recorded pid $pid is already gone — nothing to stop"
      ;;
    *)
      log "teardown: recorded pid $pid is not xcodebuild (pid-reuse guard) — not touching it"
      ;;
  esac
}

cleanup() {
  log "teardown: runners down (mini) / precise handle kill (studio) + sims shutdown + artifacts + workdir"
  if [ -n "$UDID_M" ]; then
    rssh "cd '$REMOTE_REPO' && target/release/smix runner down" || true
    rssh "cd '$REMOTE_REPO' && target/release/smix sim shutdown $UDID_M" || true
  fi
  rssh "cd '$REMOTE_REPO' && rm -rf $ARTIFACT_DIR && rm -f launch-capture.png shot-1.png shot-2.png" || true
  if [ -n "$UDID_S" ]; then
    stop_studio_runner
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

# --- 4. rebuild + freshness stamp, both nodes (cargo judges freshness;
#        the studio rebuild is what puts the C5 lane into target/release/smix) ---
log "remote rebuild (cargo build --release -p smix-cli) + stamp on $HOST"
rssh "cd '$REMOTE_REPO' && cargo build --release -p smix-cli && touch target/.smix-fed-stamp" \
  || fail "remote rebuild failed on $HOST — not running stale"
log "local rebuild (cargo build --release -p smix-cli) + stamp on studio"
( cd "$ROOT" && cargo build --release -p smix-cli && touch target/.smix-fed-stamp ) \
  || fail "local rebuild failed on studio — not running stale"

# --- 5. readiness gate, independent recheck on both nodes (same shape as
#        readiness_argv; the product lane runs its own gate again — deliberate double run) ---
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

# --- 7. the real CLI, one command through the whole lane ---
log "smix run --nodes: two nodes, one command"
cat >"$WORK/nodes.yaml" <<YAML
nodes:
  - name: c5-studio
    host: localhost
    repo: $ROOT
    devices: [$UDID_S]
    runnerPort: $STUDIO_PORT
  - name: c5-mini
    host: $HOST
    repo: $REMOTE_REPO
    devices: [$UDID_M]
YAML
( cd "$ROOT" && target/release/smix run "$FLOW_A" "$FLOW_B" \
    --nodes "$WORK/nodes.yaml" --debug-output "$WORK/pull" >"$WORK/merged.json" ) \
  || fail "smix run --nodes exited non-zero — the federation lane did not close"

# --- 8. merged report + recovered artifact assertions (python3, no jq dependency) ---
log "assert merged.json shape + per-node success leaves"
python3 - "$WORK/merged.json" <<'PY' || fail "merged.json assertions failed"
import json, sys
with open(sys.argv[1]) as f:
    text = f.read()
doc = json.loads(text)  # a single JSON document, not a line stream
assert doc["aggregateExit"] == 0, f"aggregateExit={doc['aggregateExit']}"
nodes = doc["nodes"]
assert len(nodes) == 2, f"expected 2 nodes, got {len(nodes)}"
assert {n["node"] for n in nodes} == {"c5-studio", "c5-mini"}, f"node names: {[n['node'] for n in nodes]}"
for n in nodes:
    assert len(n["flows"]) == 1, f"{n['node']}: expected exactly 1 flow leaf, got {len(n['flows'])}"
    assert n["flows"][0]["runOutcome"] == "success", f"{n['node']}: {n['flows'][0]}"
print("merged.json OK: 2 nodes, 1 success leaf each, aggregateExit 0")
PY
log "assert recovered artifacts (per-node subdirs)"
[ -f "$WORK/pull/c5-studio/run-summary.json" ] || fail "recovered artifact missing: pull/c5-studio/run-summary.json"
[ -f "$WORK/pull/c5-mini/run-summary.json" ] || fail "recovered artifact missing: pull/c5-mini/run-summary.json"

# --- 9. marker (teardown runs via trap) ---
log "C5-FED-E2E-PASS"
