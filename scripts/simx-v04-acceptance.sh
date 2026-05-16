#!/usr/bin/env bash
# simx-v04-acceptance.sh — v0.4 整体出口验收.
#
# v0.3 superset (14 fields preserved) + C2-C6 5 smoke gates + C7 grep gate
# + C8 dogfood gate = 21-field single-line JSON.  exit 0 iff PASS gate
# passes; any gated field fail → exit 1.
#
#   Phase 1 — offline 5 endpoints (swift / typecheck / TS / doctor 6 / hostid-probe)
#   Phase 2 — c6 e2e (v0.2 health + tap-text-selector + double probe)
#   Phase 3 — host-id digitizer smoke
#   Phase 4 — v0.3 endpoints (runner /tree + AXP probe + resolver e2e)
#   Phase 5 — v0.4 NEW: c2 / c3 / c4 / c5 / c6 smoke gates + C7 字面 grep
#   Phase 6 — v0.4 NEW: c8 dogfood smoke gate
#   Phase 7 — 21-field single-line JSON + aggregate exit code
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# ---- Phase 1: offline checks (no sim runner needed) -------------------

SWIFT_RAW="$(cd "$ROOT/swift-bridge" && swift test 2>&1 || true)"
SWIFT_N="$(echo "$SWIFT_RAW" \
  | grep -oE 'Executed [0-9]+ tests, with 0 failures' \
  | tail -1 | awk '{print $2}')"
SWIFT_N="${SWIFT_N:-0}"

TS_RAW="$(cd "$ROOT" && bun run test 2>&1 || true)"
TS_N="$(echo "$TS_RAW" \
  | grep -oE 'Tests +[0-9]+ passed' | head -1 | awk '{print $2}')"
TS_N="${TS_N:-0}"

TYPECHECK="ok"
(cd "$ROOT" && bun run typecheck > /dev/null 2>&1) || TYPECHECK="fail"

DOCTOR_RAW="$(cd "$ROOT" && bun src/cli/index.ts doctor 2>&1 || true)"
DOCTOR_N="$(echo "$DOCTOR_RAW" | grep -cE '^✓ ' || true)"
DOCTOR_N="${DOCTOR_N:-0}"

HOSTID_PROBE="fail"
if [[ -x "$ROOT/swift-bridge/.build/debug/simx-host-hid" ]]; then
  if "$ROOT/swift-bridge/.build/debug/simx-host-hid" probe 2>/dev/null \
      | jq -e '.ok == true' > /dev/null 2>&1; then
    HOSTID_PROBE="ok"
  fi
fi

# ---- Phase 2: c6 e2e (v0.2 compat fields) -----------------------------

C6_OUT="$(bash "$ROOT/scripts/simx-c6-tap-e2e.sh" 2>/dev/null || echo '{}')"
HEALTH="$(echo "$C6_OUT" | jq -r '.health // 0')"
CASE_PASSED="$(echo "$C6_OUT" | jq -r '.case_passed // 0')"
CASE_FAILED="$(echo "$C6_OUT" | jq -r '.case_failed // 1')"
PROBE_MISS="$(echo "$C6_OUT" | jq -r '.probe_after_miss // 0')"
PROBE_HIT="$(echo "$C6_OUT" | jq -r '.probe_after_hit // 0')"

# ---- Phase 3: hostid digitizer smoke (v0.2 compat fields) -------------

HOSTID_OUT="$(bash "$ROOT/scripts/simx-hostid-digitizer-smoke.sh" 2>/dev/null || echo '{}')"
HOSTID_HEALTH="$(echo "$HOSTID_OUT" | jq -r '.health // 0')"
HOSTID_TAP="$(echo "$HOSTID_OUT" | jq -r '.hostid_tap // "fail"')"
HOSTID_MISS="$(echo "$HOSTID_OUT" | jq -r '.probe_after_miss // 0')"
HOSTID_HIT="$(echo "$HOSTID_OUT" | jq -r '.probe_after_hit // 0')"

# ---- Phase 4: v0.3 endpoints ------------------------------------------

TREE_OUT="$(bash "$ROOT/scripts/simx-runner-tree-smoke.sh" 2>/dev/null || echo '{}')"
TREE_HEALTH="$(echo "$TREE_OUT" | jq -r '.health // 0')"
TREE_STATUS="$(echo "$TREE_OUT" | jq -r '.tree_status // 0')"
TREE_NODE_COUNT="$(echo "$TREE_OUT" | jq -r '.node_count // 0')"
TREE_GENERAL_FOUND="$(echo "$TREE_OUT" | jq -r '.general_cell_found // false')"
TREE_ROLE_BC="$(echo "$TREE_OUT" | jq -r '.role_filled_button_or_cell // false')"

AXP_OK="fail"
if [[ -x "$ROOT/swift-bridge/.build/debug/simx-host-hid" ]]; then
  if "$ROOT/swift-bridge/.build/debug/simx-host-hid" axp-probe 2>/dev/null \
      | jq -e '.ok == true' > /dev/null 2>&1; then
    AXP_OK="ok"
  fi
fi

C6R_OUT="$(bash "$ROOT/scripts/simx-c6-resolver-tap-smoke.sh" 2>/dev/null || echo '{}')"
R_ID_MISS="$(echo "$C6R_OUT" | jq -r '.id_selector_probe_miss // "fail"')"
R_ID_HIT="$(echo "$C6R_OUT" | jq -r '.id_selector_probe_hit // "fail"')"
R_ROLE_MISS="$(echo "$C6R_OUT" | jq -r '.role_selector_probe_miss // "fail"')"
R_ROLE_HIT="$(echo "$C6R_OUT" | jq -r '.role_selector_probe_hit // "fail"')"
R_TEXT_MISS="$(echo "$C6R_OUT" | jq -r '.text_selector_probe_miss // "fail"')"
R_TEXT_HIT="$(echo "$C6R_OUT" | jq -r '.text_selector_probe_hit // "fail"')"

# ---- Phase 5: v0.4 C2-C6 smoke gates + C7 字面 grep -------------------
#
# Each C2-C6 smoke shell exits 0/1; we fold to ok/fail strings. Each smoke
# brings up its own xcodebuild test session, so they cannot run in parallel
# (port 22087 contention). Sequential is fine — total runtime budget for
# the full acceptance is "minutes, not seconds" (plan-cold/v0.4.md).

C2_OUT="fail"
if bash "$ROOT/scripts/simx-c2-matcher-failure-smoke.sh" > /dev/null 2>&1; then
  C2_OUT="ok"
fi
C3_OUT="fail"
if bash "$ROOT/scripts/simx-c3-suggestions-smoke.sh" > /dev/null 2>&1; then
  C3_OUT="ok"
fi
C4_OUT="fail"
if bash "$ROOT/scripts/simx-c4-steps-jsonl-smoke.sh" > /dev/null 2>&1; then
  C4_OUT="ok"
fi
C5_OUT="fail"
if bash "$ROOT/scripts/simx-c5-step-png-smoke.sh" > /dev/null 2>&1; then
  C5_OUT="ok"
fi
C6_FAILURE_OUT="fail"
if bash "$ROOT/scripts/simx-c6-failure-json-smoke.sh" > /dev/null 2>&1; then
  C6_FAILURE_OUT="ok"
fi

# C7 has no e2e (v0.4 C7 decision 7: timing-only, unit tests sufficient).
# Verify the two close-note grep markers + the matchers.ts invert-path fix.
C7_OUT="fail"
if grep -qE "describe\('pollFor backoff \(v0\.4 C7\)" "$ROOT/src/sdk/__tests__/matchers.test.ts" \
   && grep -qE "describe\('waitFor backoff \(v0\.4 C7\)" "$ROOT/src/driver/__tests__/simctl-driver.test.ts" \
   && grep -qE 'WAITFOR_POLL_INITIAL_MS' "$ROOT/src/driver/simctl-driver.ts" \
   && grep -q 'return invert ? result : null' "$ROOT/src/sdk/matchers.ts"; then
  C7_OUT="ok"
fi

# ---- Phase 6: v0.4 C8 dogfood gate ------------------------------------

C8_OUT="fail"
C8_RAW="$(bash "$ROOT/scripts/simx-c8-dogfood-smoke.sh" 2>/dev/null || true)"
if [[ -n "$C8_RAW" ]] && echo "$C8_RAW" | jq -e '
    .typo_case_exit == 1 and .typo_json_present == "ok" and .typo_claude_general == "yes"
    and .case_case_exit == 1 and .case_json_present == "ok" and .case_claude_general == "yes"
    and .partial_case_exit == 1 and .partial_json_present == "ok" and .partial_claude_general == "yes"
   ' > /dev/null 2>&1; then
  C8_OUT="ok"
fi

# ---- Phase 7: aggregate + 21-field JSON -------------------------------

RUNNER_TAP_SMOKE="fail"
[[ "$HEALTH" == "200" ]] && RUNNER_TAP_SMOKE="ok"

HOSTID_SMOKE="fail"
if [[ "$HOSTID_HEALTH" == "200" \
      && "$HOSTID_TAP" == "ok" \
      && "$HOSTID_MISS" == "404" \
      && "$HOSTID_HIT" == "200" ]]; then
  HOSTID_SMOKE="ok"
fi

TAP_TEXT_E2E="fail"
if [[ "$CASE_PASSED" == "1" \
      && "$CASE_FAILED" == "0" \
      && "$PROBE_MISS" == "404" \
      && "$PROBE_HIT" == "200" ]]; then
  TAP_TEXT_E2E="ok"
fi

RUNNER_TREE_E2E="fail"
if [[ "$TREE_HEALTH" == "200" \
      && "$TREE_STATUS" == "200" \
      && "$TREE_NODE_COUNT" -ge 50 \
      && "$TREE_GENERAL_FOUND" == "true" \
      && "$TREE_ROLE_BC" == "true" ]]; then
  RUNNER_TREE_E2E="ok"
fi

RESOLVER_ID_E2E="fail"
if [[ "$R_ID_MISS" == "404" && "$R_ID_HIT" == "ok" ]]; then
  RESOLVER_ID_E2E="ok"
fi

# role / text raw output (NOT in PASS gate; sparse-tree regression pushed to v0.7).
RESOLVER_ROLE_E2E="info"
if [[ "$R_ROLE_MISS" == "404" && "$R_ROLE_HIT" == "ok" ]]; then
  RESOLVER_ROLE_E2E="ok"
elif [[ "$R_ROLE_MISS" == "fail" || "$R_ROLE_HIT" == "fail" ]]; then
  RESOLVER_ROLE_E2E="regression"
fi
RESOLVER_TEXT_E2E="info"
if [[ "$R_TEXT_MISS" == "404" && "$R_TEXT_HIT" == "ok" ]]; then
  RESOLVER_TEXT_E2E="ok"
elif [[ "$R_TEXT_MISS" == "fail" || "$R_TEXT_HIT" == "fail" ]]; then
  RESOLVER_TEXT_E2E="regression"
fi

PASS=0
if [[ "$SWIFT_N" -ge 102 \
      && "$TS_N" -ge 320 \
      && "$TYPECHECK" == "ok" \
      && "$DOCTOR_N" -ge 6 \
      && "$HOSTID_PROBE" == "ok" \
      && "$HEALTH" == "200" \
      && "$RUNNER_TAP_SMOKE" == "ok" \
      && "$HOSTID_SMOKE" == "ok" \
      && "$TAP_TEXT_E2E" == "ok" \
      && "$RUNNER_TREE_E2E" == "ok" \
      && "$AXP_OK" == "ok" \
      && "$RESOLVER_ID_E2E" == "ok" \
      && "$C2_OUT" == "ok" \
      && "$C3_OUT" == "ok" \
      && "$C4_OUT" == "ok" \
      && "$C5_OUT" == "ok" \
      && "$C6_FAILURE_OUT" == "ok" \
      && "$C7_OUT" == "ok" \
      && "$C8_OUT" == "ok" ]]; then
  PASS=1
fi

if [[ "$PASS" != "1" ]]; then
  {
    echo "--- v04-acceptance: gate failure ---"
    echo "swift_tests=$SWIFT_N (want >= 102)"
    echo "ts_tests=$TS_N (want >= 320)"
    echo "typecheck=$TYPECHECK (want ok)"
    echo "doctor_checks=$DOCTOR_N (want >= 6)"
    echo "hostid_probe=$HOSTID_PROBE (want ok)"
    echo "runner_health=$HEALTH (want 200)"
    echo "runner_tap_smoke=$RUNNER_TAP_SMOKE (want ok)"
    echo "hostid_digitizer_smoke=$HOSTID_SMOKE (want ok)"
    echo "tap_text_selector_e2e=$TAP_TEXT_E2E (want ok)"
    echo "runner_tree_e2e=$RUNNER_TREE_E2E (want ok; health=$TREE_HEALTH status=$TREE_STATUS node_count=$TREE_NODE_COUNT general_found=$TREE_GENERAL_FOUND role_bc=$TREE_ROLE_BC)"
    echo "axp_probe=$AXP_OK (want ok)"
    echo "resolver_id_e2e=$RESOLVER_ID_E2E (want ok; miss=$R_ID_MISS hit=$R_ID_HIT)"
    echo "resolver_role_e2e=$RESOLVER_ROLE_E2E (info-only)"
    echo "resolver_text_e2e=$RESOLVER_TEXT_E2E (info-only)"
    echo "c2_matcher_failure=$C2_OUT (want ok)"
    echo "c3_suggestions=$C3_OUT (want ok)"
    echo "c4_steps_jsonl=$C4_OUT (want ok)"
    echo "c5_step_png=$C5_OUT (want ok)"
    echo "c6_failure_json=$C6_FAILURE_OUT (want ok)"
    echo "c7_pollfor_waitfor=$C7_OUT (want ok)"
    echo "c8_dogfood=$C8_OUT (want ok; raw=$C8_RAW)"
  } >&2
fi

printf '{"swift_tests":%s,"ts_tests":%s,"typecheck":"%s","doctor_checks":%s,"hostid_probe":"%s","runner_health":%s,"runner_tap_smoke":"%s","hostid_digitizer_smoke":"%s","tap_text_selector_e2e":"%s","runner_tree_e2e":"%s","axp_probe":"%s","resolver_id_e2e":"%s","resolver_role_e2e":"%s","resolver_text_e2e":"%s","c2_matcher_failure":"%s","c3_suggestions":"%s","c4_steps_jsonl":"%s","c5_step_png":"%s","c6_failure_json":"%s","c7_pollfor_waitfor":"%s","c8_dogfood":"%s"}\n' \
  "$SWIFT_N" "$TS_N" "$TYPECHECK" "$DOCTOR_N" "$HOSTID_PROBE" \
  "$HEALTH" "$RUNNER_TAP_SMOKE" "$HOSTID_SMOKE" "$TAP_TEXT_E2E" \
  "$RUNNER_TREE_E2E" "$AXP_OK" "$RESOLVER_ID_E2E" \
  "$RESOLVER_ROLE_E2E" "$RESOLVER_TEXT_E2E" \
  "$C2_OUT" "$C3_OUT" "$C4_OUT" "$C5_OUT" "$C6_FAILURE_OUT" "$C7_OUT" "$C8_OUT"

[[ "$PASS" == "1" ]] && exit 0 || exit 1
