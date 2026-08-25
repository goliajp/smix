#!/usr/bin/env bash
# Does the live site serve this version, and are the llms files really there?
#
# Nothing in this repository deploys the site. Editing `web/` and
# committing is the whole of what the release list used to ask for, and
# the result was a site serving 2.0.0 from a July build while 3.0.0 came
# and went — two major versions of correct edits sitting in git.
#
# The second half is nastier. `smix.golia.jp` is a single-page app behind
# `try_files {path} /index.html`, so a file that is not deployed answers
# **200 with the home page**. `llms.txt` did that for its entire
# existence: fetch it and you got a success code and a document about
# nothing. A 404 would have been better — it says no.
#
# So this asks the site, and reads what came back rather than the status
# line.
#
# Usage:
#   bash scripts/release/site-is-current.sh <VERSION>
#   bash scripts/release/site-is-current.sh --selftest
set -euo pipefail

SITE="${SMIX_SITE_URL:-https://smix.golia.jp}"
# The tree this check compares the site against. Resolved from the
# script rather than the caller's cwd: the ship runs it from the repo
# root, a person may not.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# Captured, then matched. `curl | grep -q` reads as "not found" when it
# means "grep closed the pipe and curl took SIGPIPE".
has() { case "$1" in *"$2"*) return 0 ;; *) return 1 ;; esac }

is_html() { case "$1" in "<!doctype html>"*|"<!DOCTYPE html>"*|"<html"*) return 0 ;; *) return 1 ;; esac }

if [ "${1:-}" = "--selftest" ]; then
    # No network: check that the two judgements this rests on are the
    # ones written above, since both have been got wrong here before.
    fail=0
    is_html "<!doctype html>
<html>" || { echo "selftest: the SPA fallback would not be recognised" >&2; fail=1; }
    is_html "# smix" && { echo "selftest: real content misread as the fallback" >&2; fail=1; }
    has "abc 4.0.0 def" "4.0.0" || { echo "selftest: substring match is broken" >&2; fail=1; }
    has "abc 3.0.0 def" "4.0.0" && { echo "selftest: substring match too loose" >&2; fail=1; }
    [ "$fail" = 0 ] || exit 1
    echo "site-is-current selftest: the fallback is recognised and the version match is exact"
    exit 0
fi

VERSION="${1:-}"
[ -n "$VERSION" ] || { echo "usage: site-is-current.sh <VERSION> | --selftest" >&2; exit 2; }

fail=0

INDEX="$(curl -fsS --max-time 20 "$SITE" 2>/dev/null || true)"
if [ -z "$INDEX" ]; then
    echo "site-is-current: $SITE did not answer — cannot say whether it is current" >&2
    exit 1
fi

ASSET="$(printf '%s' "$INDEX" | grep -oE 'assets/index-[A-Za-z0-9_-]+\.js' | head -1)"
if [ -z "$ASSET" ]; then
    echo "site-is-current: no bundled asset in the served page — the site's shape \
changed and this check is reading air" >&2
    exit 1
fi

BUNDLE="$(curl -fsS --max-time 30 "$SITE/$ASSET" 2>/dev/null || true)"
if has "$BUNDLE" "$VERSION"; then
    echo "site-is-current: $SITE serves $VERSION"
else
    echo "site-is-current: FAIL — $SITE does not mention $VERSION." >&2
    echo "  The site is deployed by hand: rsync -av --delete web/dist/ \
t01:/var/lib/smix-web/ . Nothing in this repo does it for you, which is how \
it came to sit two major versions behind." >&2
    fail=1
fi

for f in llms.txt llms-full.txt; do
    BODY="$(curl -fsS --max-time 20 "$SITE/$f" 2>/dev/null || true)"
    if [ -z "$BODY" ]; then
        echo "site-is-current: FAIL — $SITE/$f returned nothing" >&2
        fail=1
    elif is_html "$BODY"; then
        echo "site-is-current: FAIL — $SITE/$f answers with the home page. The \
SPA fallback turns a missing file into a 200, so a client asking for it gets a \
success code and a page about nothing." >&2
        fail=1
    elif ! has "$BODY" "smix"; then
        echo "site-is-current: FAIL — $SITE/$f does not look like the generated \
file" >&2
        fail=1
    elif [ -f "$ROOT/$f" ] && \
         [ "$(printf '%s' "$BODY" | shasum -a 256 | cut -d' ' -f1)" \
           != "$(printf '%s' "$(cat "$ROOT/$f")" | shasum -a 256 | cut -d' ' -f1)" ]; then
        # Both sides go through a command substitution on purpose: it
        # strips trailing newlines, and comparing a stripped body with an
        # unstripped file reported a one-byte difference as the site
        # being behind. A measurement artefact and a real finding look
        # identical from the exit code.
        #
        # Naming the right version is not the same as being current. A
        # selector-guide fix landed the day after 8.0.0 shipped and the
        # site went on serving the generation before it — the version
        # coordinate was right, so nothing here said a word. The whole
        # point of that fix was that a reader stops losing an afternoon,
        # and an undeployed fix helps nobody who reads the site.
        echo "site-is-current: FAIL — $SITE/$f is not what this tree generates \
(served $(printf '%s' "$BODY" | wc -c | tr -d ' ') bytes, tree has \
$(wc -c < "$ROOT/$f" | tr -d ' ')). Rebuild web/ and rsync — see the release \
checklist, doc upgrade." >&2
        fail=1
    else
        echo "site-is-current: $SITE/$f is the real file, and is what this tree generates"
    fi
done

exit "$fail"
