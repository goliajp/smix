#!/usr/bin/env bash
# Did the registries actually take it?
#
# `ship.sh` printed "SHIP COMPLETE — live on crates.io + npm + Maven
# Central + Swift Package" from its own control flow, having asked none
# of them. The sentence was not merely unverified, it was wrong at the
# moment it was printed: Maven Central took three hours to publish
# 6.5.0, so the ship said "live on Maven Central" about something that
# would not be there until the evening.
#
# This asks. Every publishable crate rather than a sample, every npm
# package including the per-triple ones, the Swift tag on the remote,
# and Maven's own metadata plus the artifacts beside it.
#
# Maven is allowed to be late and is never allowed to be claimed. Its
# propagation is minutes to hours with no way to hurry it, so an absent
# artifact there is reported as NOT YET rather than as a failure — and
# the summary says which channels were confirmed rather than asserting
# all of them.
#
# Verifies the version THIS TREE just shipped. Asked about an older
# one it reads the current crate list, which may have grown since —
# running it for 6.5.0 after smix-contract was added reports that crate
# missing, correctly and uselessly.
#
# Usage:
#   scripts/release/verify-published.sh <version>
#   SMIX_VERIFY_SKIP_MAVEN=1 scripts/release/verify-published.sh 6.5.0

set -uo pipefail

VERSION="${1:-}"
[ -n "$VERSION" ] || { echo "usage: verify-published.sh <version>" >&2; exit 2; }
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

FAILED=0
CONFIRMED=()
PENDING=()

say() { printf 'verify: %s\n' "$*"; }
bad() { printf 'verify: FAIL %s\n' "$*" >&2; FAILED=1; }

# --- crates.io --------------------------------------------------------
# From cargo metadata, not a list: a list is a second place to forget a
# crate, and the publish DAG already has one gate for that.
CRATES="$(cd "$ROOT" && cargo metadata --no-deps --format-version 1 \
  | python3 -c 'import json,sys
print("\n".join(sorted(p["name"] for p in json.load(sys.stdin)["packages"] if p.get("publish") != [])))')"
CR_OK=0; CR_TOTAL=0; CR_MISSING=""
while read -r c; do
  [ -z "$c" ] && continue
  CR_TOTAL=$((CR_TOTAL + 1))
  case ${#c} in
    1) path="1/$c" ;;
    2) path="2/$c" ;;
    3) path="3/${c:0:1}/$c" ;;
    *) path="${c:0:2}/${c:2:2}/$c" ;;
  esac
  if curl -sf "https://index.crates.io/$path" 2>/dev/null | grep -q "\"vers\":\"$VERSION\""; then
    CR_OK=$((CR_OK + 1))
  else
    CR_MISSING="$CR_MISSING $c"
  fi
done <<< "$CRATES"
if [ "$CR_OK" -eq "$CR_TOTAL" ] && [ "$CR_TOTAL" -gt 0 ]; then
  say "crates.io $CR_OK/$CR_TOTAL at $VERSION"
  CONFIRMED+=("crates.io")
else
  bad "crates.io $CR_OK/$CR_TOTAL —$CR_MISSING"
fi

# --- npm --------------------------------------------------------------
# The per-triple subpackages included: the parent resolving is not the
# same as a user on that triple getting a binary.
# `private: true` is npm's own way of saying "never publish this", and
# it is why @goliapkg/smix-web-record is in this tree and not on the
# registry. The first draft of this asked about it anyway and reported
# a package missing that was never meant to be there — a verifier that
# invents a failure is as unusable as one that invents a success.
NPM_PKGS="$(cd "$ROOT" && python3 -c '
import json, glob, os

def add(names, path):
    d = json.load(open(path))
    if d.get("private"):
        return
    names.append(d["name"])
    for sub in glob.glob(os.path.join(os.path.dirname(path), "npm", "*", "package.json")):
        add(names, sub)

names = []
for f in glob.glob("npm/*/package.json") + glob.glob("crates/*/package.json"):
    add(names, f)
print("\n".join(sorted(set(names))))')"
NPM_OK=0; NPM_TOTAL=0; NPM_MISSING=""
while read -r p; do
  [ -z "$p" ] && continue
  NPM_TOTAL=$((NPM_TOTAL + 1))
  got="$(npm view "$p" version 2>/dev/null)"
  if [ "$got" = "$VERSION" ]; then
    NPM_OK=$((NPM_OK + 1))
  else
    NPM_MISSING="$NPM_MISSING $p(${got:-absent})"
  fi
done <<< "$NPM_PKGS"
if [ "$NPM_OK" -eq "$NPM_TOTAL" ] && [ "$NPM_TOTAL" -gt 0 ]; then
  say "npm $NPM_OK/$NPM_TOTAL at $VERSION"
  CONFIRMED+=("npm")
else
  bad "npm $NPM_OK/$NPM_TOTAL —$NPM_MISSING"
fi

# --- Swift ------------------------------------------------------------
if git -C "$ROOT" ls-remote --tags origin 2>/dev/null | grep -q "refs/tags/swift-v$VERSION\$"; then
  say "swift tag swift-v$VERSION on the remote"
  CONFIRMED+=("Swift tag")
else
  bad "no swift-v$VERSION tag on the remote"
fi

# --- Maven Central ----------------------------------------------------
# Late is normal; claimed is not. 6.5.0 took three hours.
if [ "${SMIX_VERIFY_SKIP_MAVEN:-0}" = 1 ]; then
  say "maven — skipped by request"
else
  # Which artifacts, read off the ship's publish task rather than listed
  # again here. `smix-probe` shipped for a release before this line was
  # derived and nothing asked about it — a second copy of a list is the one
  # that goes stale, and here the stale half is the half that verifies.
  ARTIFACTS=()
  while IFS= read -r a; do ARTIFACTS+=("$a"); done < <(
    grep -oE '^GRADLE_PUB_TASKS=\(.*\)' "$(dirname "${BASH_SOURCE[0]}")/ship.sh" \
      | head -1 | grep -oE ':[a-z-]+:publish' | sed 's/^://; s/:publish$//' \
      | sed 's/^sdk$/smix-sdk/; s/^probe$/smix-probe/'
  )
  if [ ${#ARTIFACTS[@]} -eq 0 ]; then
    say "maven — could not read the publish list out of ship.sh, so this would"
    say "  be verifying a list of nothing"
    FAILED=1
  fi
  MAVEN_OK=1
  for ART in "${ARTIFACTS[@]}"; do
    M="https://repo1.maven.org/maven2/jp/golia/smix/$ART"
    REL="$(curl -sf "$M/maven-metadata.xml" 2>/dev/null | sed -n 's:.*<release>\(.*\)</release>.*:\1:p' | head -1)"
    AAR="$(curl -s -o /dev/null -w '%{http_code}' "$M/$VERSION/$ART-$VERSION.aar" 2>/dev/null)"
    ASC="$(curl -s -o /dev/null -w '%{http_code}' "$M/$VERSION/$ART-$VERSION.aar.asc" 2>/dev/null)"
    if [ "$REL" = "$VERSION" ] && [ "$AAR" = "200" ] && [ "$ASC" = "200" ]; then
      say "maven central $ART <release>=$VERSION, aar and asc both 200"
    else
      say "maven central $ART NOT YET — <release>=${REL:-unknown}, aar=$AAR, asc=$ASC"
      MAVEN_OK=0
    fi
  done
  if [ "$MAVEN_OK" = 1 ] && [ ${#ARTIFACTS[@]} -gt 0 ]; then
    CONFIRMED+=("Maven Central (${#ARTIFACTS[@]} artifacts)")
  else
    say "  (propagation is minutes to hours and cannot be hurried; re-run this"
    say "   later. It is not claimed below until every artifact answers.)"
    PENDING+=("Maven Central")
  fi
fi

echo
if [ "$FAILED" -eq 0 ] && [ ${#PENDING[@]} -eq 0 ]; then
  say "v$VERSION confirmed on: ${CONFIRMED[*]}"
  exit 0
fi
if [ "$FAILED" -eq 0 ]; then
  say "v$VERSION confirmed on: ${CONFIRMED[*]}"
  say "still to come: ${PENDING[*]} — re-run this until it answers"
  exit 0
fi
say "v$VERSION is NOT fully published. Confirmed: ${CONFIRMED[*]:-none}"
exit 1
