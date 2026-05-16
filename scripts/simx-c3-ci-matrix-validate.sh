#!/usr/bin/env bash
# v0.7 C3 — GitHub Actions CI workflow matrix contract validator.
# Regex-asserts `.github/workflows/ci.yml` carries the multi-runtime
# matrix shape (iOS-17-5 / iOS-18-4 / iOS-26-4 + fail-fast: false +
# continue-on-error guard on non-26.4 branches) on top of the 7 C2 tokens.
# Emits a 14-field JSON gate (mirror of scripts/simx-c2-ci-workflow-validate.sh
# shape; +6 fields for matrix surface, -2 fields no_matrix/single_job
# which are inherently invalidated by the C3 matrix introduction).
#
# This validator does NOT execute the 5 CI gate commands — those are
# verified in the Checkpoint C3 acceptance block separately. The intent
# here is workflow-file shape, not pipeline replay.
#
# Note: scripts/simx-c2-ci-workflow-validate.sh is intentionally left
# in place and unmodified; after the C3 ci.yml edit it will report
# no_matrix=fail / single_job=fail. That is the honest expression of
# the C2->C3 boundary transition. C3 acceptance does not call C2
# validator.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

YML=".github/workflows/ci.yml"

has_macos_15="fail"
has_setup_bun="fail"
has_bun_install_frozen="fail"
has_typecheck="fail"
has_vitest="fail"
has_swift_test="fail"
has_mcp_smoke="fail"
has_matrix="fail"
has_runtime_17_5="fail"
has_runtime_18_4="fail"
has_runtime_26_4="fail"
has_fail_fast_false="fail"
has_continue_on_error="fail"

if [[ -f "$YML" ]]; then
  grep -qE '^[[:space:]]*runs-on: macos-15$' "$YML" && has_macos_15="ok"
  grep -qE 'oven-sh/setup-bun@v2' "$YML" && has_setup_bun="ok"
  grep -qE 'bun install --frozen-lockfile' "$YML" && has_bun_install_frozen="ok"
  grep -qE 'bun run typecheck' "$YML" && has_typecheck="ok"
  grep -qE 'bun x vitest run' "$YML" && has_vitest="ok"
  grep -qE '(^|[[:space:]])swift test([[:space:]]|$)' "$YML" && has_swift_test="ok"
  grep -qE 'simx-c1-mcp-smoke\.sh' "$YML" && has_mcp_smoke="ok"
  if grep -qE '^[[:space:]]*strategy:[[:space:]]*$' "$YML" \
       && grep -qE '^[[:space:]]*matrix:[[:space:]]*$' "$YML"; then
    has_matrix="ok"
  fi
  grep -qE 'iOS-17-5' "$YML" && has_runtime_17_5="ok"
  grep -qE 'iOS-18-4' "$YML" && has_runtime_18_4="ok"
  grep -qE 'iOS-26-4' "$YML" && has_runtime_26_4="ok"
  grep -qE 'fail-fast:[[:space:]]*false' "$YML" && has_fail_fast_false="ok"
  grep -qE "continue-on-error:.*matrix\\.runtime.*!=.*'iOS-26-4'" "$YML" \
    && has_continue_on_error="ok"
fi

EXIT_CODE=0
ALL_OK="ok"
for v in "$has_macos_15" "$has_setup_bun" "$has_bun_install_frozen" \
         "$has_typecheck" "$has_vitest" "$has_swift_test" \
         "$has_mcp_smoke" "$has_matrix" "$has_runtime_17_5" \
         "$has_runtime_18_4" "$has_runtime_26_4" "$has_fail_fast_false" \
         "$has_continue_on_error"; do
  if [[ "$v" != "ok" ]]; then ALL_OK="fail"; EXIT_CODE=1; fi
done

printf '{"exit_code":%d,"has_macos_15":"%s","has_setup_bun":"%s","has_bun_install_frozen":"%s","has_typecheck":"%s","has_vitest":"%s","has_swift_test":"%s","has_mcp_smoke":"%s","has_matrix":"%s","has_runtime_17_5":"%s","has_runtime_18_4":"%s","has_runtime_26_4":"%s","has_fail_fast_false":"%s","has_continue_on_error":"%s","all_ok":"%s"}\n' \
  "$EXIT_CODE" "$has_macos_15" "$has_setup_bun" "$has_bun_install_frozen" \
  "$has_typecheck" "$has_vitest" "$has_swift_test" "$has_mcp_smoke" \
  "$has_matrix" "$has_runtime_17_5" "$has_runtime_18_4" "$has_runtime_26_4" \
  "$has_fail_fast_false" "$has_continue_on_error" "$ALL_OK"

exit "$EXIT_CODE"
