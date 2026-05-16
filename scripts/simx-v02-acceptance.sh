#!/usr/bin/env bash
# simx-v02-acceptance.sh — v0.2 整体出口验收。
#
# 串联 v0.2 全部 5 个 checkpoint 的机器可判断 endpoint，stdout 单行
# JSON（9 个固定字段），exit 0 = v0.2 整体验收通过；任一 gate fail →
# exit 1 + 对应字段为 "fail"/0/非期望值。
#
#   Phase 1 — 离线 5 endpoint：swift test / TS test / typecheck /
#             doctor 5-check / simx-host-hid probe
#   Phase 2 — c6-e2e（runner health 200 + tap-text-selector e2e
#             passed + 双面 probe 404/200）
#   Phase 3 — hostid digitizer smoke（host-hid IOHIDEvent digitizer
#             tap 真 UI 跳页）
#   Phase 4 — 单行 JSON summary + aggregate exit code
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# ---- Phase 1: offline checks (no sim runner needed) -------------------

# swift test (~10s when cached).
SWIFT_RAW="$(cd "$ROOT/swift-bridge" && swift test 2>&1 || true)"
SWIFT_N="$(echo "$SWIFT_RAW" \
  | grep -oE 'Executed [0-9]+ tests, with 0 failures' \
  | tail -1 | awk '{print $2}')"
SWIFT_N="${SWIFT_N:-0}"

# bun run test (~1s).
TS_RAW="$(cd "$ROOT" && bun run test 2>&1 || true)"
TS_N="$(echo "$TS_RAW" \
  | grep -oE 'Tests +[0-9]+ passed' | head -1 | awk '{print $2}')"
TS_N="${TS_N:-0}"

# typecheck (~3s).
TYPECHECK="ok"
(cd "$ROOT" && bun run typecheck > /dev/null 2>&1) || TYPECHECK="fail"

# doctor (~3s; counts "✓ " lines).
DOCTOR_RAW="$(cd "$ROOT" && bun src/cli/index.ts doctor 2>&1 || true)"
DOCTOR_N="$(echo "$DOCTOR_RAW" | grep -cE '^✓ ' || true)"
DOCTOR_N="${DOCTOR_N:-0}"

# simx-host-hid probe (~1s; binary already built by c6 deps).
HOSTID_PROBE="fail"
if [[ -x "$ROOT/swift-bridge/.build/debug/simx-host-hid" ]]; then
  if "$ROOT/swift-bridge/.build/debug/simx-host-hid" probe 2>/dev/null \
      | jq -e '.ok == true' > /dev/null 2>&1; then
    HOSTID_PROBE="ok"
  fi
fi

# ---- Phase 2: c6 e2e (auto-starts runner, drives example, probes UI) --
# Output schema: {"health":200,"case_passed":1,"case_failed":0,
#                 "probe_after_miss":404,"probe_after_hit":200}
C6_OUT="$(bash "$ROOT/scripts/simx-c6-tap-e2e.sh" 2>/dev/null || echo '{}')"
HEALTH="$(echo "$C6_OUT" | jq -r '.health // 0')"
CASE_PASSED="$(echo "$C6_OUT" | jq -r '.case_passed // 0')"
CASE_FAILED="$(echo "$C6_OUT" | jq -r '.case_failed // 1')"
PROBE_MISS="$(echo "$C6_OUT" | jq -r '.probe_after_miss // 0')"
PROBE_HIT="$(echo "$C6_OUT" | jq -r '.probe_after_hit // 0')"

# ---- Phase 3: hostid digitizer smoke (runner re-start internal) -------
# Output schema: {"health":200,"hostid_tap":"ok",
#                 "probe_after_miss":404,"probe_after_hit":200}
HOSTID_OUT="$(bash "$ROOT/scripts/simx-hostid-digitizer-smoke.sh" 2>/dev/null || echo '{}')"
HOSTID_HEALTH="$(echo "$HOSTID_OUT" | jq -r '.health // 0')"
HOSTID_TAP="$(echo "$HOSTID_OUT" | jq -r '.hostid_tap // "fail"')"
HOSTID_MISS="$(echo "$HOSTID_OUT" | jq -r '.probe_after_miss // 0')"
HOSTID_HIT="$(echo "$HOSTID_OUT" | jq -r '.probe_after_hit // 0')"

# ---- Phase 4: aggregate + single-line JSON ----------------------------

# runner_tap_smoke 字段 pass = runner_health 200（C2 /tap 验证由 c6
# Phase D/E probe 子集证伪：MISS 404 + HIT 200，所以这里只 gate health）
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

PASS=0
# Counts use >= so the script stays green as future versions add tests on top
# of the v0.2 baseline (63 swift / 125 TS).
if [[ "$SWIFT_N" -ge 63 \
      && "$TS_N" -ge 125 \
      && "$TYPECHECK" == "ok" \
      && "$DOCTOR_N" -ge 5 \
      && "$HOSTID_PROBE" == "ok" \
      && "$HEALTH" == "200" \
      && "$RUNNER_TAP_SMOKE" == "ok" \
      && "$HOSTID_SMOKE" == "ok" \
      && "$TAP_TEXT_E2E" == "ok" ]]; then
  PASS=1
fi

# Diagnostic tail to stderr if any gate failed.
if [[ "$PASS" != "1" ]]; then
  {
    echo "--- v02-acceptance: gate failure ---"
    echo "swift_tests=$SWIFT_N (want >= 63)"
    echo "ts_tests=$TS_N (want >= 125)"
    echo "typecheck=$TYPECHECK (want ok)"
    echo "doctor_checks=$DOCTOR_N (want >= 5)"
    echo "hostid_probe=$HOSTID_PROBE (want ok)"
    echo "runner_health=$HEALTH (want 200)"
    echo "runner_tap_smoke=$RUNNER_TAP_SMOKE (want ok)"
    echo "hostid_digitizer_smoke=$HOSTID_SMOKE (want ok)"
    echo "tap_text_selector_e2e=$TAP_TEXT_E2E (want ok)"
    echo "--- c6_out=$C6_OUT"
    echo "--- hostid_out=$HOSTID_OUT"
  } >&2
fi

printf '{"swift_tests":%s,"ts_tests":%s,"typecheck":"%s","doctor_checks":%s,"hostid_probe":"%s","runner_health":%s,"runner_tap_smoke":"%s","hostid_digitizer_smoke":"%s","tap_text_selector_e2e":"%s"}\n' \
  "$SWIFT_N" "$TS_N" "$TYPECHECK" "$DOCTOR_N" "$HOSTID_PROBE" \
  "$HEALTH" "$RUNNER_TAP_SMOKE" "$HOSTID_SMOKE" "$TAP_TEXT_E2E"

[[ "$PASS" == "1" ]] && exit 0 || exit 1
