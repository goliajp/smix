#!/usr/bin/env bash
# v0.7 C2 — GitHub Actions CI workflow file contract validator.
# Regex-asserts `.github/workflows/ci.yml` carries the 9 literal tokens
# that the single-runtime CI gate depends on. Emits a 10-field JSON gate
# (mirror of scripts/simx-c[1-8]-*.sh shape).
#
# This validator does NOT execute the 5 CI gate commands — those are
# verified in the Checkpoint C2 acceptance block separately. The intent
# here is workflow-file shape, not pipeline replay.
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
no_matrix="fail"
single_job="fail"

if [[ -f "$YML" ]]; then
  grep -qE '^[[:space:]]*runs-on: macos-15$' "$YML" && has_macos_15="ok"
  grep -qE 'oven-sh/setup-bun@v2' "$YML" && has_setup_bun="ok"
  grep -qE 'bun install --frozen-lockfile' "$YML" && has_bun_install_frozen="ok"
  grep -qE 'bun run typecheck' "$YML" && has_typecheck="ok"
  grep -qE 'bun x vitest run' "$YML" && has_vitest="ok"
  grep -qE '(^|[[:space:]])swift test([[:space:]]|$)' "$YML" && has_swift_test="ok"
  grep -qE 'simx-c1-mcp-smoke\.sh' "$YML" && has_mcp_smoke="ok"
  if ! grep -qE '^[[:space:]]*(strategy:|matrix:)' "$YML"; then no_matrix="ok"; fi
  # single_job = exactly one top-level key under `jobs:` (line `  <key>:` at 2-space indent under jobs).
  JOB_COUNT="$(awk '/^jobs:/{f=1;next} f && /^[a-zA-Z]/ {f=0} f && /^  [a-zA-Z0-9_-]+:[[:space:]]*$/ {c++} END{print c+0}' "$YML")"
  test "$JOB_COUNT" = "1" && single_job="ok"
fi

EXIT_CODE=0
ALL_OK="ok"
for v in "$has_macos_15" "$has_setup_bun" "$has_bun_install_frozen" \
         "$has_typecheck" "$has_vitest" "$has_swift_test" \
         "$has_mcp_smoke" "$no_matrix" "$single_job"; do
  if [[ "$v" != "ok" ]]; then ALL_OK="fail"; EXIT_CODE=1; fi
done

printf '{"exit_code":%d,"has_macos_15":"%s","has_setup_bun":"%s","has_bun_install_frozen":"%s","has_typecheck":"%s","has_vitest":"%s","has_swift_test":"%s","has_mcp_smoke":"%s","no_matrix":"%s","single_job":"%s","all_ok":"%s"}\n' \
  "$EXIT_CODE" "$has_macos_15" "$has_setup_bun" "$has_bun_install_frozen" \
  "$has_typecheck" "$has_vitest" "$has_swift_test" "$has_mcp_smoke" \
  "$no_matrix" "$single_job" "$ALL_OK"

exit "$EXIT_CODE"
