#!/usr/bin/env bash
# v2.13-C1 CLI distribution e2e: install smix from packed npm tarballs into
# an empty directory, with no Rust on PATH, and drive both executables.
#
# The claim this defends is the one a new user meets first: getting smix
# should not require a Rust toolchain. `cargo install smix-cli --locked`
# compiles 27 crates and asks for rustup from someone whose app is Swift or
# Kotlin, and that is the whole of the install story today.
#
# Testing it by running the binary in this repo would prove nothing — the
# binary is already built here. So this packs the real tarballs, installs
# them somewhere empty, runs them on a PATH built without cargo or rustc,
# and requires both executables to work from there.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PKG="$ROOT/npm/smix-cli"

log()  { printf '[c1-dist] %s\n' "$*"; }
fail() { printf '[c1-dist] FAIL: %s\n' "$*" >&2; exit 1; }

# --- host triple ---------------------------------------------------------

case "$(uname -s)/$(uname -m)" in
  Darwin/arm64)  SUFFIX="darwin-arm64" ;;
  Darwin/x86_64) SUFFIX="darwin-x64" ;;
  Linux/x86_64)  SUFFIX="linux-x64-gnu" ;;
  *) fail "no packaged target for $(uname -s)/$(uname -m)" ;;
esac
log "host package: @goliapkg/smix-cli-$SUFFIX"

WORK="$(mktemp -d)"
cleanup() {
  # The staged binaries live inside the repo's package dir; leaving them
  # behind would make the next pack ship whatever was there.
  rm -f "$PKG/npm/$SUFFIX/smix" "$PKG/npm/$SUFFIX/smix-mcp"
  rm -rf "$WORK"
}
trap cleanup EXIT

# --- build + stage -------------------------------------------------------

log "build host binaries"
( cd "$ROOT" && cargo build --release -p smix-cli -p smix-mcp ) \
  || fail "cargo build failed"

for exe in smix smix-mcp; do
  [ -x "$ROOT/target/release/$exe" ] || fail "missing build output: $exe"
  cp "$ROOT/target/release/$exe" "$PKG/npm/$SUFFIX/$exe"
done

log "compile launcher sources"
( cd "$PKG" && bun x tsc ) || fail "tsc failed"

log "pack both packages"
( cd "$PKG" && bun pm pack --destination "$WORK" >/dev/null ) \
  || fail "pack (main) failed"
( cd "$PKG/npm/$SUFFIX" && bun pm pack --destination "$WORK" >/dev/null ) \
  || fail "pack (platform) failed"

MAIN_TGZ="$(ls "$WORK"/goliapkg-smix-cli-[0-9]*.tgz 2>/dev/null | head -1)"
PLAT_TGZ="$(ls "$WORK"/goliapkg-smix-cli-"$SUFFIX"-*.tgz 2>/dev/null | head -1)"
[ -n "$MAIN_TGZ" ] || fail "main tarball not produced"
[ -n "$PLAT_TGZ" ] || fail "platform tarball not produced"

# --- clean room ----------------------------------------------------------

# A real node, because the installed shims start with `#!/usr/bin/env node`
# and that is what someone who installed an npm package has. On this machine
# node comes from nvm and is a shell function, so a non-interactive shell
# sees nothing — find the binary rather than assume the name resolves.
NODE_BIN="$(command -v node 2>/dev/null || true)"
if [ ! -x "${NODE_BIN:-}" ]; then
  NODE_BIN="$(ls -d "$HOME"/.nvm/versions/node/*/bin/node 2>/dev/null | sort -V | tail -1 || true)"
fi
[ -x "${NODE_BIN:-}" ] || fail "no node binary found; the installed bin shims need one"

# Built rather than filtered: a PATH that still carries cargo cannot tell a
# working package from one that quietly fell back to a source build.
CLEAN_PATH="$(dirname "$NODE_BIN"):/usr/bin:/bin:/usr/sbin:/sbin"
env PATH="$CLEAN_PATH" sh -c 'command -v cargo >/dev/null 2>&1' \
  && fail "cargo is still reachable in the clean-room PATH"

CONSUMER="$WORK/consumer"
mkdir -p "$CONSUMER"
printf '{"name":"c1-consumer","version":"0.0.0","private":true}\n' > "$CONSUMER/package.json"

log "install from tarballs into an empty project"
( cd "$CONSUMER" && bun add "$PLAT_TGZ" "$MAIN_TGZ" >"$WORK/install.log" 2>&1 ) \
  || { tail -20 "$WORK/install.log" >&2; fail "install of the tarballs failed"; }

log "smix --version, with no cargo or rustc reachable"
VERSION_OUT="$(cd "$CONSUMER" && env PATH="$CLEAN_PATH" ./node_modules/.bin/smix --version 2>&1)" \
  || fail "smix --version failed: $VERSION_OUT"

EXPECTED="$(cd "$ROOT" && cargo metadata --no-deps --format-version 1 \
  | python3 -c 'import json,sys
m=json.load(sys.stdin)
print(next(p["version"] for p in m["packages"] if p["name"]=="smix-cli"))')"
case "$VERSION_OUT" in
  *"$EXPECTED"*) log "version matches the workspace: $EXPECTED" ;;
  *) fail "version mismatch — package said '$VERSION_OUT', workspace is $EXPECTED" ;;
esac

# The MCP server is the half of this package Claude Code talks to, and it
# speaks over stdio — so the check is a real initialize handshake, not
# --version. A launcher that buffered or wrote its own line into these
# streams would pass a version check and corrupt this one.
log "smix-mcp answers an MCP initialize over stdio"
REQ='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"c1-dist","version":"0"}}}'
MCP_OUT="$(cd "$CONSUMER" && printf '%s\n' "$REQ" \
  | env PATH="$CLEAN_PATH" ./node_modules/.bin/smix-mcp 2>/dev/null \
  | head -1 || true)"
case "$MCP_OUT" in
  *'"result"'*'"serverInfo"'*) log "initialize answered" ;;
  *) fail "smix-mcp did not answer initialize; got: ${MCP_OUT:-<nothing>}" ;;
esac

log "C1-CLI-DIST-PASS"
