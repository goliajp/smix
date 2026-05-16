#!/usr/bin/env bash
# simx-c4-doctor-smoke.sh — v0.7 C4 doctor JSON schema gate (upgraded
# from v0.5 C4 6-field shape to v0.7 C4 9-field shape including
# compatibility-status coverage).
#
# Real-run `simx doctor --json`, parse the output, and emit a 9-field
# single-line JSON gate result:
#   {exit_code, checks_len, schema_ok, runtimes_has_items,
#    claude_logged_in, has_compatibility_field, compat_status_enum_ok,
#    runtimes_compat_supported, all_ok}
#
# Exit 0 iff every gated field is "ok"; non-zero iff any field fails.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

RAW="$(bun src/cli/index.ts doctor --json 2>/dev/null || echo '{}')"

EXIT_CODE="$(echo "$RAW" | jq -r '.exitCode // -1')"
CHECKS_LEN="$(echo "$RAW" | jq -r '.checks | length' 2>/dev/null || echo 0)"
CHECKS_LEN="${CHECKS_LEN:-0}"

# Schema gate: 6 names in fixed order + every check has name+ok+(value xor message).
NAMES="$(echo "$RAW" | jq -c '.checks | map(.name)' 2>/dev/null || echo '[]')"
SCHEMA_OK="fail"
if [[ "$NAMES" == '["xcode","runtimes","claude","bun","hid","axp"]' ]]; then
  BAD="$(echo "$RAW" | jq '[
    .checks[] |
    select(
      (.name | type) != "string"
      or (.ok | type) != "boolean"
      or (.value == null and .message == null)
    )
  ] | length' 2>/dev/null || echo 99)"
  [[ "$BAD" == "0" ]] && SCHEMA_OK="ok"
fi

# runtimes value.items is a non-empty array.
RUNTIMES_HAS_ITEMS="fail"
ITEMS_LEN="$(echo "$RAW" | jq '.checks[1].value.items | length' 2>/dev/null || echo 0)"
ITEMS_LEN="${ITEMS_LEN:-0}"
[[ "$ITEMS_LEN" -ge 1 ]] && RUNTIMES_HAS_ITEMS="ok"

# claude.value.loggedIn === true.
CLAUDE_LOGGED_IN="fail"
LOGGED_IN="$(echo "$RAW" | jq -r '.checks[2].value.loggedIn // false' 2>/dev/null || echo false)"
[[ "$LOGGED_IN" == "true" ]] && CLAUDE_LOGGED_IN="ok"

# v0.7 C4 — compatibility field present on every check.
HAS_COMPAT="fail"
MISSING_COMPAT="$(echo "$RAW" | jq '[.checks[] | select(.compatibility == null)] | length' 2>/dev/null || echo 99)"
[[ "$MISSING_COMPAT" == "0" ]] && HAS_COMPAT="ok"

# v0.7 C4 — every compatibility.status in locked 4-enum.
COMPAT_STATUS_OK="fail"
BAD_STATUS="$(echo "$RAW" | jq '[
  .checks[]
  | .compatibility.status
  | select(. != "supported" and . != "unsupported" and . != "partial" and . != "unknown")
] | length' 2>/dev/null || echo 99)"
[[ "$BAD_STATUS" == "0" ]] && COMPAT_STATUS_OK="ok"

# v0.7 C4 — runtimes.compatibility.status === "supported" on hosts that
# have at least one iOS-26 runtime installed (CI macos-15 + dev baseline).
RUNTIMES_COMPAT_SUPPORTED="fail"
RT_STATUS="$(echo "$RAW" | jq -r '.checks[1].compatibility.status // "fail"' 2>/dev/null || echo fail)"
[[ "$RT_STATUS" == "supported" ]] && RUNTIMES_COMPAT_SUPPORTED="ok"

ALL_OK="fail"
if [[ "$EXIT_CODE" == "0" \
      && "$CHECKS_LEN" == "6" \
      && "$SCHEMA_OK" == "ok" \
      && "$RUNTIMES_HAS_ITEMS" == "ok" \
      && "$CLAUDE_LOGGED_IN" == "ok" \
      && "$HAS_COMPAT" == "ok" \
      && "$COMPAT_STATUS_OK" == "ok" \
      && "$RUNTIMES_COMPAT_SUPPORTED" == "ok" ]]; then
  ALL_OK="ok"
fi

printf '{"exit_code":%s,"checks_len":%s,"schema_ok":"%s","runtimes_has_items":"%s","claude_logged_in":"%s","has_compatibility_field":"%s","compat_status_enum_ok":"%s","runtimes_compat_supported":"%s","all_ok":"%s"}\n' \
  "$EXIT_CODE" "$CHECKS_LEN" "$SCHEMA_OK" "$RUNTIMES_HAS_ITEMS" "$CLAUDE_LOGGED_IN" \
  "$HAS_COMPAT" "$COMPAT_STATUS_OK" "$RUNTIMES_COMPAT_SUPPORTED" "$ALL_OK"

[[ "$ALL_OK" == "ok" ]] && exit 0 || exit 1
