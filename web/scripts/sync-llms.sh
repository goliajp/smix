#!/usr/bin/env bash
# Put the generated llms files where vite will serve them.
#
# They were never on the site. `smix.golia.jp/llms.txt` answered 200 with
# the single-page app's HTML, because Caddy's `try_files {path}
# /index.html` turns every miss into the home page — so a client fetching
# the file got a success code and a document about nothing. A 404 would
# have been better: it at least says no.
#
# Copied at build time rather than committed: `gen-llms.py` owns them,
# and a second copy in the repo is a second answer that drifts.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
mkdir -p "$ROOT/web/public"
for f in llms.txt llms-full.txt; do
    if [ ! -f "$ROOT/$f" ]; then
        echo "sync-llms: $ROOT/$f is missing — run scripts/dev/gen-llms.py" >&2
        exit 1
    fi
    cp "$ROOT/$f" "$ROOT/web/public/$f"
done
