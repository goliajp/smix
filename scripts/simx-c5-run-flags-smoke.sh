#!/usr/bin/env bash
# simx-c5-run-flags-smoke.sh — v0.5 C5 `simx run --json` JSON CI-schema gate.
# Real-run `simx run examples/screenshot-only.test.ts --json`, parse the
# output, and emit a single-line 8-field JSON gate result:
#   {exit_code, json_parseable, passed_field, failed_field, total_field,
#    cases_array_present, case_schema, all_ok}
#
# Exit 0 iff every gated field is "ok"; non-zero iff any field fails.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

FIXTURE="examples/screenshot-only.test.ts"
if [[ ! -f "$FIXTURE" ]]; then
  echo "missing fixture: $FIXTURE" >&2
  exit 1
fi

# Lock to dev sim. v0.6 audit (2026-05-16): default pickCell selects the first
# booted device which could be the user's own sim. Explicitly pass dev-sim
# UDID from .simx/dev-sim.txt so dev-lock's assertDevSimLock never fires.
DEV_SIM_UDID=""
if [[ -f "$ROOT/.simx/dev-sim.txt" ]]; then
  DEV_SIM_UDID="$(tr -d '[:space:]' < "$ROOT/.simx/dev-sim.txt")"
fi

if [[ -n "$DEV_SIM_UDID" ]]; then
  RAW="$(bun src/cli/index.ts run "$FIXTURE" --json --udid "$DEV_SIM_UDID" 2>/dev/null || echo '{}')"
else
  RAW="$(bun src/cli/index.ts run "$FIXTURE" --json 2>/dev/null || echo '{}')"
fi

JSON_PARSEABLE="fail"
if echo "$RAW" | jq -e '.' > /dev/null 2>&1; then
  JSON_PARSEABLE="ok"
fi

EXIT_CODE="$(echo "$RAW" | jq -r '.exitCode // -1')"
PASSED="$(echo "$RAW" | jq -r '.passed // -1')"
FAILED="$(echo "$RAW" | jq -r '.failed // -1')"
TOTAL="$(echo "$RAW" | jq -r '.total // -1')"
CASES_LEN="$(echo "$RAW" | jq -r '.cases | length' 2>/dev/null || echo -1)"
CASES_LEN="${CASES_LEN:--1}"

PASSED_FIELD="fail"
[[ "$PASSED" -ge 1 ]] && PASSED_FIELD="ok"

FAILED_FIELD="fail"
[[ "$FAILED" -ge 0 ]] && FAILED_FIELD="ok"

TOTAL_FIELD="fail"
[[ "$TOTAL" -ge 1 ]] && TOTAL_FIELD="ok"

CASES_ARRAY_PRESENT="fail"
[[ "$CASES_LEN" -ge 1 ]] && CASES_ARRAY_PRESENT="ok"

# Each case must have name:string + status:string + durationMs:number.
CASE_SCHEMA="fail"
BAD="$(echo "$RAW" | jq '[.cases[] | select((.name | type) != "string" or (.status | type) != "string" or (.durationMs | type) != "number")] | length' 2>/dev/null || echo -1)"
[[ "$BAD" == "0" ]] && CASE_SCHEMA="ok"

ALL_OK="fail"
if [[ "$EXIT_CODE" == "0" \
   && "$JSON_PARSEABLE" == "ok" \
   && "$PASSED_FIELD" == "ok" \
   && "$FAILED_FIELD" == "ok" \
   && "$TOTAL_FIELD" == "ok" \
   && "$CASES_ARRAY_PRESENT" == "ok" \
   && "$CASE_SCHEMA" == "ok" ]]; then
  ALL_OK="ok"
fi

printf '{"exit_code":%s,"json_parseable":"%s","passed_field":"%s","failed_field":"%s","total_field":"%s","cases_array_present":"%s","case_schema":"%s","all_ok":"%s"}\n' \
  "$EXIT_CODE" "$JSON_PARSEABLE" "$PASSED_FIELD" "$FAILED_FIELD" "$TOTAL_FIELD" "$CASES_ARRAY_PRESENT" "$CASE_SCHEMA" "$ALL_OK"

[[ "$ALL_OK" == "ok" ]] && exit 0 || exit 1
