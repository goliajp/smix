#!/usr/bin/env bash
# Install a released smix on another Mac over ssh, the way a user there
# would, and say whether it took.
#
#   bash scripts/release/install-on-host.sh <ssh-host> <version>
#
# The host is somebody's machine running flows of its own; the CLI is what a
# run there uses, and an older one keeps the defects a release fixed. npm is
# the path: it brings smix and smix-mcp together and builds nothing on the
# host. The command runs in a login shell, because a plain ssh session's
# PATH has neither npm nor what npm installs into.
#
# The app's probe is not installed here: it is built into the app, and the
# app's own build decides its version.
set -euo pipefail

HOST="${1:?usage: install-on-host.sh <ssh-host> <version>}"
VERSION="${2:?usage: install-on-host.sh <ssh-host> <version>}"

log()  { printf 'install-on-host: %s\n' "$*" >&2; }
fail() { printf 'install-on-host: FAIL: %s\n' "$*" >&2; exit 1; }

case "$VERSION" in
  [0-9]*.[0-9]*.[0-9]*) : ;;
  *) fail "'$VERSION' is not a version" ;;
esac

log "installing @goliapkg/smix-cli@$VERSION on $HOST"
ssh -o BatchMode=yes "$HOST" "zsh -lc 'npm install -g @goliapkg/smix-cli@$VERSION'" >&2 \
  || fail "npm install on $HOST failed"

got="$(ssh -o BatchMode=yes "$HOST" "zsh -lc 'smix --version; smix-mcp --version'")" \
  || fail "smix on $HOST does not run"
want="smix $VERSION
smix-mcp $VERSION"
[ "$got" = "$want" ] || fail "$HOST answers \"$(printf '%s' "$got" | tr '\n' ' ')\", not $VERSION for both"
log "$HOST runs smix $VERSION and smix-mcp $VERSION"
