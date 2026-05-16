#!/usr/bin/env bash
# simx-v03-acceptance.sh — v0.3 整体出口验收。
#
# 串联 v0.2 的 9 字段 + v0.3 新增 3 gated + 2 info = 14 字段单行 JSON。
# exit 0 = v0.3 整体验收通过；任一 gated field fail → exit 1。
#
#   Phase 1 — 离线 5 endpoint（swift / typecheck / TS / doctor 6 / hostid-probe）
#   Phase 2 — c6-e2e（runner /health 200 + tap-text-selector 真 UI 跳页 + 双面 probe）
#   Phase 3 — hostid digitizer smoke
#   Phase 4 — v0.3 NEW: runner /tree + AXP probe + resolver id/role/text selector e2e
#   Phase 5 — 单行 JSON summary + aggregate exit code
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

# ---- Phase 2: c6 e2e (v02 兼容字段) -----------------------------------

C6_OUT="$(bash "$ROOT/scripts/simx-c6-tap-e2e.sh" 2>/dev/null || echo '{}')"
HEALTH="$(echo "$C6_OUT" | jq -r '.health // 0')"
CASE_PASSED="$(echo "$C6_OUT" | jq -r '.case_passed // 0')"
CASE_FAILED="$(echo "$C6_OUT" | jq -r '.case_failed // 1')"
PROBE_MISS="$(echo "$C6_OUT" | jq -r '.probe_after_miss // 0')"
PROBE_HIT="$(echo "$C6_OUT" | jq -r '.probe_after_hit // 0')"

# ---- Phase 3: hostid digitizer smoke (v02 兼容字段) -------------------

HOSTID_OUT="$(bash "$ROOT/scripts/simx-hostid-digitizer-smoke.sh" 2>/dev/null || echo '{}')"
HOSTID_HEALTH="$(echo "$HOSTID_OUT" | jq -r '.health // 0')"
HOSTID_TAP="$(echo "$HOSTID_OUT" | jq -r '.hostid_tap // "fail"')"
HOSTID_MISS="$(echo "$HOSTID_OUT" | jq -r '.probe_after_miss // 0')"
HOSTID_HIT="$(echo "$HOSTID_OUT" | jq -r '.probe_after_hit // 0')"

# ---- Phase 4: v0.3 NEW endpoints --------------------------------------

# 4.1 runner /tree e2e
# simx-runner-tree-smoke.sh exits non-zero unless: app rawType=application,
# identifier matches the expected bundle, root bounds.w/h>0 (jq gate inside
# the smoke script), General cell found, node_count>=10, role-fill works.
# Its summary JSON exposes node_count + general_cell_found + role gates;
# we re-gate them here for clarity.
TREE_OUT="$(bash "$ROOT/scripts/simx-runner-tree-smoke.sh" 2>/dev/null || echo '{}')"
TREE_HEALTH="$(echo "$TREE_OUT" | jq -r '.health // 0')"
TREE_STATUS="$(echo "$TREE_OUT" | jq -r '.tree_status // 0')"
TREE_NODE_COUNT="$(echo "$TREE_OUT" | jq -r '.node_count // 0')"
TREE_GENERAL_FOUND="$(echo "$TREE_OUT" | jq -r '.general_cell_found // false')"
TREE_ROLE_BC="$(echo "$TREE_OUT" | jq -r '.role_filled_button_or_cell // false')"

# 4.2 AXP probe
AXP_OK="fail"
if [[ -x "$ROOT/swift-bridge/.build/debug/simx-host-hid" ]]; then
  if "$ROOT/swift-bridge/.build/debug/simx-host-hid" axp-probe 2>/dev/null \
      | jq -e '.ok == true' > /dev/null 2>&1; then
    AXP_OK="ok"
  fi
fi

# 4.3 c6-resolver-tap-smoke (id phase = gate; role/text info-only)
C6R_OUT="$(bash "$ROOT/scripts/simx-c6-resolver-tap-smoke.sh" 2>/dev/null || echo '{}')"
R_ID_MISS="$(echo "$C6R_OUT" | jq -r '.id_selector_probe_miss // "fail"')"
R_ID_HIT="$(echo "$C6R_OUT" | jq -r '.id_selector_probe_hit // "fail"')"
R_ROLE_MISS="$(echo "$C6R_OUT" | jq -r '.role_selector_probe_miss // "fail"')"
R_ROLE_HIT="$(echo "$C6R_OUT" | jq -r '.role_selector_probe_hit // "fail"')"
R_TEXT_MISS="$(echo "$C6R_OUT" | jq -r '.text_selector_probe_miss // "fail"')"
R_TEXT_HIT="$(echo "$C6R_OUT" | jq -r '.text_selector_probe_hit // "fail"')"

# ---- Phase 5: aggregate + 14-field JSON -------------------------------

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

# role / text raw output (NOT in PASS gate; sparse-tree regression 推 v0.7).
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
      && "$TS_N" -ge 264 \
      && "$TYPECHECK" == "ok" \
      && "$DOCTOR_N" -ge 6 \
      && "$HOSTID_PROBE" == "ok" \
      && "$HEALTH" == "200" \
      && "$RUNNER_TAP_SMOKE" == "ok" \
      && "$HOSTID_SMOKE" == "ok" \
      && "$TAP_TEXT_E2E" == "ok" \
      && "$RUNNER_TREE_E2E" == "ok" \
      && "$AXP_OK" == "ok" \
      && "$RESOLVER_ID_E2E" == "ok" ]]; then
  PASS=1
fi

# Diagnostic tail to stderr if any gate failed.
if [[ "$PASS" != "1" ]]; then
  {
    echo "--- v03-acceptance: gate failure ---"
    echo "swift_tests=$SWIFT_N (want >= 102)"
    echo "ts_tests=$TS_N (want >= 264)"
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
    echo "resolver_role_e2e=$RESOLVER_ROLE_E2E (info-only; miss=$R_ROLE_MISS hit=$R_ROLE_HIT)"
    echo "resolver_text_e2e=$RESOLVER_TEXT_E2E (info-only; miss=$R_TEXT_MISS hit=$R_TEXT_HIT)"
    echo "--- c6_out=$C6_OUT"
    echo "--- hostid_out=$HOSTID_OUT"
    echo "--- tree_out=$TREE_OUT"
    echo "--- c6r_out=$C6R_OUT"
  } >&2
fi

printf '{"swift_tests":%s,"ts_tests":%s,"typecheck":"%s","doctor_checks":%s,"hostid_probe":"%s","runner_health":%s,"runner_tap_smoke":"%s","hostid_digitizer_smoke":"%s","tap_text_selector_e2e":"%s","runner_tree_e2e":"%s","axp_probe":"%s","resolver_id_e2e":"%s","resolver_role_e2e":"%s","resolver_text_e2e":"%s"}\n' \
  "$SWIFT_N" "$TS_N" "$TYPECHECK" "$DOCTOR_N" "$HOSTID_PROBE" \
  "$HEALTH" "$RUNNER_TAP_SMOKE" "$HOSTID_SMOKE" "$TAP_TEXT_E2E" \
  "$RUNNER_TREE_E2E" "$AXP_OK" "$RESOLVER_ID_E2E" \
  "$RESOLVER_ROLE_E2E" "$RESOLVER_TEXT_E2E"

[[ "$PASS" == "1" ]] && exit 0 || exit 1
