#!/usr/bin/env bash
# simx-v05-acceptance.sh — v0.5 整体出口验收.
#
# v0.4 21-field superset (wrapped, byte-identical names) + 6 new v0.5 gates
# (C1 repl startup / C2 history-undo-redo / C3 save codegen / C4 doctor /
# C5 run flags / C6 boot command) = 27-field single-line JSON. exit 0 iff
# every gated field passes the PASS gate.
#
#   Phase A — wrap scripts/simx-v04-acceptance.sh → extract 21 fields
#   Phase B — 6 v0.5 gates (C1/C2 inline REPL gates; C3/C4/C5/C6 smoke wraps)
#   Phase C — 27-field single-line JSON + aggregate exit code
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 99

# Lock to dev sim by name. v0.6 audit (2026-05-16): hardcoded 'iPhone 17 Pro'
# matched the user's own sim (FDBDF6F3-...) instead of simx-dev (7B88259F-...),
# so dev-lock now correctly rejects the c1/c2/c5 acceptance gates. Use the dev
# sim's name string to keep these gates accurate to the locked sim only.
DEVICE_NAME="simx-dev"

# ---- Phase A: wrap v0.4 acceptance (21 fields) ------------------------

V04_RAW="$(bash "$ROOT/scripts/simx-v04-acceptance.sh" 2>/dev/null || echo '{}')"
V04_LINE="$(echo "$V04_RAW" | grep '^{' | tail -1)"
[[ -z "$V04_LINE" ]] && V04_LINE='{}'

SWIFT_TESTS="$(echo "$V04_LINE" | jq -r '.swift_tests // 0')"
TS_TESTS="$(echo "$V04_LINE" | jq -r '.ts_tests // 0')"
TYPECHECK="$(echo "$V04_LINE" | jq -r '.typecheck // "fail"')"
DOCTOR_CHECKS="$(echo "$V04_LINE" | jq -r '.doctor_checks // 0')"
HOSTID_PROBE="$(echo "$V04_LINE" | jq -r '.hostid_probe // "fail"')"
RUNNER_HEALTH="$(echo "$V04_LINE" | jq -r '.runner_health // 0')"
RUNNER_TAP_SMOKE="$(echo "$V04_LINE" | jq -r '.runner_tap_smoke // "fail"')"
HOSTID_DIGITIZER_SMOKE="$(echo "$V04_LINE" | jq -r '.hostid_digitizer_smoke // "fail"')"
TAP_TEXT_SELECTOR_E2E="$(echo "$V04_LINE" | jq -r '.tap_text_selector_e2e // "fail"')"
RUNNER_TREE_E2E="$(echo "$V04_LINE" | jq -r '.runner_tree_e2e // "fail"')"
AXP_PROBE="$(echo "$V04_LINE" | jq -r '.axp_probe // "fail"')"
RESOLVER_ID_E2E="$(echo "$V04_LINE" | jq -r '.resolver_id_e2e // "fail"')"
RESOLVER_ROLE_E2E="$(echo "$V04_LINE" | jq -r '.resolver_role_e2e // "info"')"
RESOLVER_TEXT_E2E="$(echo "$V04_LINE" | jq -r '.resolver_text_e2e // "info"')"
C2_MATCHER_FAILURE="$(echo "$V04_LINE" | jq -r '.c2_matcher_failure // "fail"')"
C3_SUGGESTIONS="$(echo "$V04_LINE" | jq -r '.c3_suggestions // "fail"')"
C4_STEPS_JSONL="$(echo "$V04_LINE" | jq -r '.c4_steps_jsonl // "fail"')"
C5_STEP_PNG="$(echo "$V04_LINE" | jq -r '.c5_step_png // "fail"')"
C6_FAILURE_JSON="$(echo "$V04_LINE" | jq -r '.c6_failure_json // "fail"')"
C7_POLLFOR_WAITFOR="$(echo "$V04_LINE" | jq -r '.c7_pollfor_waitfor // "fail"')"
C8_DOGFOOD="$(echo "$V04_LINE" | jq -r '.c8_dogfood // "fail"')"

# ---- Phase B: 6 v0.5 gates --------------------------------------------

# B1 — C1 REPL startup: `.help` + `.exit` exits 0, stdout contains "commands:"
C1_REPL_STARTUP="fail"
C1_OUT="$(printf '.help\n.exit\n' | bun src/cli/index.ts repl --device "$DEVICE_NAME" 2>&1 || true)"
C1_EXIT=$?
if [[ "$C1_EXIT" == "0" ]] && echo "$C1_OUT" | grep -qE 'commands:'; then
  C1_REPL_STARTUP="ok"
fi

# B2 — C2 history / undo / redo: meta verbs print expected empty-state markers
C2_HISTORY_UNDO_REDO="fail"
C2_OUT="$(printf '.history\n.undo\n.redo\n.exit\n' | bun src/cli/index.ts repl --device "$DEVICE_NAME" 2>&1 || true)"
C2_EXIT=$?
if [[ "$C2_EXIT" == "0" ]] \
   && echo "$C2_OUT" | grep -qE 'no history yet' \
   && echo "$C2_OUT" | grep -qE 'nothing to undo' \
   && echo "$C2_OUT" | grep -qE 'nothing to redo'; then
  C2_HISTORY_UNDO_REDO="ok"
fi

# B3 — C3 save codegen smoke
C3_SAVE_CODEGEN="fail"
if bash "$ROOT/scripts/simx-c3-save-e2e-smoke.sh" 2>/dev/null \
   | jq -e '.tsc_ok == "ok"' > /dev/null 2>&1; then
  C3_SAVE_CODEGEN="ok"
fi

# B4 — C4 doctor checks deep smoke
C4_DOCTOR_CHECKS_DEEP="fail"
if bash "$ROOT/scripts/simx-c4-doctor-smoke.sh" 2>/dev/null \
   | jq -e '.all_ok == "ok"' > /dev/null 2>&1; then
  C4_DOCTOR_CHECKS_DEEP="ok"
fi

# B5 — C5 run flags smoke
C5_RUN_FLAGS="fail"
if bash "$ROOT/scripts/simx-c5-run-flags-smoke.sh" 2>/dev/null \
   | jq -e '.all_ok == "ok"' > /dev/null 2>&1; then
  C5_RUN_FLAGS="ok"
fi

# B6 — C6 boot command smoke
C6_BOOT_COMMAND="fail"
if bash "$ROOT/scripts/simx-c6-boot-smoke.sh" 2>/dev/null \
   | jq -e '.all_ok == "ok"' > /dev/null 2>&1; then
  C6_BOOT_COMMAND="ok"
fi

# ---- Phase C: PASS gate + 27-field JSON -------------------------------

PASS=0
if [[ "$SWIFT_TESTS" -ge 102 \
      && "$TS_TESTS" -ge 419 \
      && "$TYPECHECK" == "ok" \
      && "$DOCTOR_CHECKS" -ge 6 \
      && "$HOSTID_PROBE" == "ok" \
      && "$RUNNER_HEALTH" == "200" \
      && "$RUNNER_TAP_SMOKE" == "ok" \
      && "$HOSTID_DIGITIZER_SMOKE" == "ok" \
      && "$TAP_TEXT_SELECTOR_E2E" == "ok" \
      && "$RUNNER_TREE_E2E" == "ok" \
      && "$AXP_PROBE" == "ok" \
      && "$RESOLVER_ID_E2E" == "ok" \
      && "$C2_MATCHER_FAILURE" == "ok" \
      && "$C3_SUGGESTIONS" == "ok" \
      && "$C4_STEPS_JSONL" == "ok" \
      && "$C5_STEP_PNG" == "ok" \
      && "$C6_FAILURE_JSON" == "ok" \
      && "$C7_POLLFOR_WAITFOR" == "ok" \
      && "$C8_DOGFOOD" == "ok" \
      && "$C1_REPL_STARTUP" == "ok" \
      && "$C2_HISTORY_UNDO_REDO" == "ok" \
      && "$C3_SAVE_CODEGEN" == "ok" \
      && "$C4_DOCTOR_CHECKS_DEEP" == "ok" \
      && "$C5_RUN_FLAGS" == "ok" \
      && "$C6_BOOT_COMMAND" == "ok" ]]; then
  PASS=1
fi

if [[ "$PASS" != "1" ]]; then
  {
    echo "--- v05-acceptance: gate failure ---"
    echo "(v0.4 superset)"
    echo "swift_tests=$SWIFT_TESTS ts_tests=$TS_TESTS typecheck=$TYPECHECK doctor_checks=$DOCTOR_CHECKS"
    echo "hostid_probe=$HOSTID_PROBE runner_health=$RUNNER_HEALTH runner_tap_smoke=$RUNNER_TAP_SMOKE"
    echo "hostid_digitizer_smoke=$HOSTID_DIGITIZER_SMOKE tap_text_selector_e2e=$TAP_TEXT_SELECTOR_E2E"
    echo "runner_tree_e2e=$RUNNER_TREE_E2E axp_probe=$AXP_PROBE resolver_id_e2e=$RESOLVER_ID_E2E"
    echo "c2_matcher_failure=$C2_MATCHER_FAILURE c3_suggestions=$C3_SUGGESTIONS c4_steps_jsonl=$C4_STEPS_JSONL"
    echo "c5_step_png=$C5_STEP_PNG c6_failure_json=$C6_FAILURE_JSON c7_pollfor_waitfor=$C7_POLLFOR_WAITFOR"
    echo "c8_dogfood=$C8_DOGFOOD"
    echo "(v0.5 new)"
    echo "c1_repl_startup=$C1_REPL_STARTUP"
    echo "c2_history_undo_redo=$C2_HISTORY_UNDO_REDO"
    echo "c3_save_codegen=$C3_SAVE_CODEGEN"
    echo "c4_doctor_checks_deep=$C4_DOCTOR_CHECKS_DEEP"
    echo "c5_run_flags=$C5_RUN_FLAGS"
    echo "c6_boot_command=$C6_BOOT_COMMAND"
  } >&2
fi

printf '{"swift_tests":%s,"ts_tests":%s,"typecheck":"%s","doctor_checks":%s,"hostid_probe":"%s","runner_health":%s,"runner_tap_smoke":"%s","hostid_digitizer_smoke":"%s","tap_text_selector_e2e":"%s","runner_tree_e2e":"%s","axp_probe":"%s","resolver_id_e2e":"%s","resolver_role_e2e":"%s","resolver_text_e2e":"%s","c2_matcher_failure":"%s","c3_suggestions":"%s","c4_steps_jsonl":"%s","c5_step_png":"%s","c6_failure_json":"%s","c7_pollfor_waitfor":"%s","c8_dogfood":"%s","c1_repl_startup":"%s","c2_history_undo_redo":"%s","c3_save_codegen":"%s","c4_doctor_checks_deep":"%s","c5_run_flags":"%s","c6_boot_command":"%s"}\n' \
  "$SWIFT_TESTS" "$TS_TESTS" "$TYPECHECK" "$DOCTOR_CHECKS" "$HOSTID_PROBE" \
  "$RUNNER_HEALTH" "$RUNNER_TAP_SMOKE" "$HOSTID_DIGITIZER_SMOKE" "$TAP_TEXT_SELECTOR_E2E" \
  "$RUNNER_TREE_E2E" "$AXP_PROBE" "$RESOLVER_ID_E2E" \
  "$RESOLVER_ROLE_E2E" "$RESOLVER_TEXT_E2E" \
  "$C2_MATCHER_FAILURE" "$C3_SUGGESTIONS" "$C4_STEPS_JSONL" "$C5_STEP_PNG" \
  "$C6_FAILURE_JSON" "$C7_POLLFOR_WAITFOR" "$C8_DOGFOOD" \
  "$C1_REPL_STARTUP" "$C2_HISTORY_UNDO_REDO" "$C3_SAVE_CODEGEN" \
  "$C4_DOCTOR_CHECKS_DEEP" "$C5_RUN_FLAGS" "$C6_BOOT_COMMAND"

[[ "$PASS" == "1" ]] && exit 0 || exit 1
