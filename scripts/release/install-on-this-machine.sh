#!/usr/bin/env bash
# Put a published version on this machine, then ask whether it landed.
#
# Both binaries, because they are separate crates and only useful
# together; any global npm install already on the machine; every Claude
# profile that has the plugin, because
# `claude plugin update` moves only the profile it runs under. The
# judgement is this-machine-is-current.sh's, not this script's: what was
# installed is not evidence of what is installed.
#
# From the registries, not the workspace build — this is the machine
# using the release the way anyone else would.
#
# A plugin update applies at the next session start; sessions already
# running keep the version they loaded.
#
# Usage:
#   bash scripts/release/install-on-this-machine.sh <VERSION>
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VERSION="${1:-}"
[ -n "$VERSION" ] || { echo "usage: install-on-this-machine.sh <VERSION>" >&2; exit 2; }

cargo install "smix-cli@$VERSION" "smix-mcp@$VERSION" --locked

# A global npm install is a second copy on PATH, and a shell that loads
# nvm finds it before ~/.cargo/bin: 11.0.0 was installed through cargo
# while the npm copy from five days earlier kept answering as 10.0.0. Each
# node that already has the package is moved to this version; a node
# without it is left alone.
for node_bin in "$HOME"/.nvm/versions/node/*/bin; do
  [ -x "$node_bin/npm" ] || continue
  if PATH="$node_bin:$PATH" npm ls -g --depth=0 @goliapkg/smix-cli >/dev/null 2>&1; then
    echo "install-on-this-machine: updating the global npm install under $node_bin"
    PATH="$node_bin:$PATH" npm install -g "@goliapkg/smix-cli@$VERSION"
  fi
done

while IFS= read -r profile; do
  echo "install-on-this-machine: updating the plugin in $profile"
  CLAUDE_CONFIG_DIR="$profile" claude plugin marketplace update goliajp
  CLAUDE_CONFIG_DIR="$profile" claude plugin update smix@goliajp
done < <(bash "$HERE/this-machine-is-current.sh" --profiles)

bash "$HERE/this-machine-is-current.sh" "$VERSION"
