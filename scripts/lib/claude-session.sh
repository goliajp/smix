#!/usr/bin/env bash
# Whether a `claude -p` session that a plugin e2e started ran to its end.
#
# Source this, do not run it. A session that could not start, or that the
# service stopped partway (a usage limit, an expired login), produced no
# evidence about the plugin either way: judging its tool calls reads a
# half-told story as a failure of the product.
#
#   claude_session_unrunnable <file>...   text output names a reason the
#                                         session could not run
#   claude_stream_cut_short <stream.jsonl> for `--output-format stream-json`:
#                                         prints why and succeeds when the
#                                         run did not finish (the final
#                                         `result` record reports an error,
#                                         or there is none)
#
#     bash scripts/lib/claude-session.sh --selftest

CLAUDE_SESSION_UNRUNNABLE='reached your .* limit|hit your .* limit|/usage-credits|not logged in|Invalid API key|command not found|credit balance'

claude_session_unrunnable() {
  grep -qiE "$CLAUDE_SESSION_UNRUNNABLE" "$@" 2>/dev/null
}

claude_stream_cut_short() {
  python3 - "$1" <<'PY'
import json, sys

result = None
for line in open(sys.argv[1], encoding="utf-8", errors="replace"):
    line = line.strip()
    if not line:
        continue
    try:
        msg = json.loads(line)
    except json.JSONDecodeError:
        continue
    if msg.get("type") == "result":
        result = msg
if result is None:
    print("the stream has no final result record — the session was cut off")
    sys.exit(0)
if result.get("is_error"):
    why = result.get("terminal_reason") or result.get("subtype") or "error"
    print(f"{why}: {str(result.get('result', '')).strip()}")
    sys.exit(0)
sys.exit(1)
PY
}

if [ "${BASH_SOURCE[0]}" = "$0" ] && [ "${1:-}" = "--selftest" ]; then
  set -u
  fail() { echo "claude-session selftest FAIL: $*"; exit 1; }
  T="$(mktemp -d)"
  trap 'rm -rf "$T"' EXIT

  printf '%s\n' '{"type":"system","subtype":"init"}' \
    '{"type":"result","subtype":"success","is_error":true,"terminal_reason":"api_error","result":"You'"'"'ve hit your session limit · resets 8pm"}' > "$T/limit.jsonl"
  why="$(claude_stream_cut_short "$T/limit.jsonl")" || fail "a run the service stopped read as finished"
  case "$why" in *api_error*"session limit"*) ;; *) fail "the reason was not named: $why" ;; esac

  printf '%s\n' '{"type":"system","subtype":"init"}' > "$T/cut.jsonl"
  claude_stream_cut_short "$T/cut.jsonl" >/dev/null || fail "a stream with no result read as finished"

  printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"done"}' > "$T/ok.jsonl"
  if claude_stream_cut_short "$T/ok.jsonl" >/dev/null; then fail "a finished run read as cut short"; fi

  printf "You've hit your session limit · resets 8pm\n" > "$T/limit.txt"
  claude_session_unrunnable "$T/limit.txt" || fail "the session-limit wording was not recognised"
  printf 'the guard refused it\n' > "$T/ran.txt"
  if claude_session_unrunnable "$T/ran.txt"; then fail "an ordinary transcript read as unrunnable"; fi

  echo "claude-session: selftest ok"
fi
