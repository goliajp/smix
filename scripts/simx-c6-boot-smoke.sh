#!/usr/bin/env bash
# simx-c6-boot-smoke.sh — v0.5 C6 `simx boot <udid>` JSON CI-schema gate.
#
# Real-run `bun src/cli/index.ts boot <DEV_UDID> --json`, parse the
# output, and emit a single-line 8-field JSON gate result:
#   {exit_code, json_parseable, udid_field, state_field,
#    already_booted_field, duration_field, name_field, all_ok}
#
# Note: dev sim is already booted at smoke time, so this exercises the
# alreadyBooted=true noop path. The bootAndWait path is covered by mocks
# in src/cli/__tests__/boot.test.ts (decision 7).
#
# Exit 0 iff every gated field is "ok"; non-zero iff any field fails.
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 99

DEV_UDID="$(tr -d '[:space:]' < .simx/dev-sim.txt 2>/dev/null || echo "")"
if [[ -z "$DEV_UDID" ]]; then
  echo '{"error":"no dev-sim udid","all_ok":"fail"}'
  exit 1
fi

RAW="$(bun src/cli/index.ts boot "$DEV_UDID" --json 2>&1)"
EXIT_CODE=$?

JSON_PARSEABLE="fail"
echo "$RAW" | jq -e . > /dev/null 2>&1 && JSON_PARSEABLE="ok"

UDID_FIELD="fail"
[[ "$(echo "$RAW" | jq -r '.udid // empty' 2>/dev/null)" == "$DEV_UDID" ]] && UDID_FIELD="ok"

STATE_FIELD="fail"
[[ "$(echo "$RAW" | jq -r '.state // empty' 2>/dev/null)" == "Booted" ]] && STATE_FIELD="ok"

ALREADY_BOOTED_FIELD="fail"
[[ "$(echo "$RAW" | jq -r '.alreadyBooted // false' 2>/dev/null)" == "true" ]] && ALREADY_BOOTED_FIELD="ok"

DURATION_FIELD="fail"
DUR="$(echo "$RAW" | jq -r '.durationMs // -1' 2>/dev/null)"
if [[ "$DUR" =~ ^[0-9]+$ ]] && (( DUR >= 0 )); then
  DURATION_FIELD="ok"
fi

NAME_FIELD="fail"
NAME_VAL="$(echo "$RAW" | jq -r '.name // empty' 2>/dev/null)"
[[ -n "$NAME_VAL" ]] && NAME_FIELD="ok"

ALL_OK="fail"
if [[ "$EXIT_CODE" == "0" \
   && "$JSON_PARSEABLE" == "ok" \
   && "$UDID_FIELD" == "ok" \
   && "$STATE_FIELD" == "ok" \
   && "$ALREADY_BOOTED_FIELD" == "ok" \
   && "$DURATION_FIELD" == "ok" \
   && "$NAME_FIELD" == "ok" ]]; then
  ALL_OK="ok"
fi

printf '{"exit_code":%s,"json_parseable":"%s","udid_field":"%s","state_field":"%s","already_booted_field":"%s","duration_field":"%s","name_field":"%s","all_ok":"%s"}\n' \
  "$EXIT_CODE" "$JSON_PARSEABLE" "$UDID_FIELD" "$STATE_FIELD" "$ALREADY_BOOTED_FIELD" "$DURATION_FIELD" "$NAME_FIELD" "$ALL_OK"

[[ "$ALL_OK" == "ok" ]] && exit 0 || exit 1
