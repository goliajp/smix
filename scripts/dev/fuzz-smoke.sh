#!/usr/bin/env bash
# Short fuzz pass over every fuzz target in the workspace.
#
# 15 targets existed with nothing running them — a fuzz target that
# never runs is decoration. This is not a soak: each target gets a
# small time budget (FUZZ_SECONDS, default 20), enough to catch the
# panics-on-malformed-input class that these parsers exist to refuse.
# Longer soaks stay a manual `cargo +nightly fuzz run <target>`.
#
# Usage: scripts/dev/fuzz-smoke.sh [crate-dir ...]
#   FUZZ_SECONDS=60 scripts/dev/fuzz-smoke.sh crates/smix-selector

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SECONDS_PER_TARGET="${FUZZ_SECONDS:-20}"

command -v cargo-fuzz >/dev/null 2>&1 \
  || { echo "fuzz-smoke: cargo-fuzz not installed — cargo install cargo-fuzz" >&2; exit 1; }
rustup toolchain list | grep -q nightly \
  || { echo "fuzz-smoke: no nightly toolchain — rustup toolchain install nightly" >&2; exit 1; }

# cargo-fuzz's default target on macOS is x86_64-apple-darwin, which on
# an arm64 host with no cross std fails at `can't find crate for std`.
HOST_TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"

if [[ $# -gt 0 ]]; then
  CRATES=("$@")
else
  CRATES=()
  for d in "$ROOT"/crates/*/fuzz; do
    [[ -d "$d" ]] && CRATES+=("$(dirname "$d")")
  done
fi

failed=()
for crate in "${CRATES[@]}"; do
  for target_file in "$crate"/fuzz/fuzz_targets/*.rs; do
    target="$(basename "$target_file" .rs)"
    name="$(basename "$crate")/$target"
    echo "fuzz-smoke: $name (${SECONDS_PER_TARGET}s)"
    if ! (cd "$crate" && cargo +nightly fuzz run "$target" \
        --target "$HOST_TRIPLE" -- \
        -max_total_time="$SECONDS_PER_TARGET" -print_final_stats=0) \
        > "/tmp/smix-fuzz-$target.log" 2>&1; then
      # Tell "the target found something" from "the toolchain moved
      # under us".
      #
      # cargo-fuzz runs `rustc --version` and requires it to begin with
      # "rustc". While rustup is replacing a toolchain it does not, and
      # cargo-fuzz exits before compiling anything with a one-line log
      # that reads exactly like a broken target. Measured on 2026-08-22:
      # this machine's rustup updated nightly to 1.100.0 at 03:03 and a
      # ship an hour later died here, on a target that passes by hand
      # and passed on the next run of the same script.
      #
      # A retry is the right answer for this one and only this one: the
      # apparatus was unavailable, not the subject wrong.
      if grep -q "Rust version string does not start with" \
           "/tmp/smix-fuzz-$target.log" 2>/dev/null; then
        echo "fuzz-smoke: $name — rustc was unreadable (rustup mid-update?); retrying once"
        if (cd "$crate" && cargo +nightly fuzz run "$target" \
            --target "$HOST_TRIPLE" -- \
            -max_total_time="$SECONDS_PER_TARGET" -print_final_stats=0) \
            > "/tmp/smix-fuzz-$target.log" 2>&1; then
          continue
        fi
      fi
      failed+=("$name (/tmp/smix-fuzz-$target.log)")
    fi
  done
done

if [[ ${#failed[@]} -gt 0 ]]; then
  echo "fuzz-smoke: ${#failed[@]} target(s) FAILED:"
  printf '  %s\n' "${failed[@]}"
  exit 1
fi
echo "fuzz-smoke: all targets clean at ${SECONDS_PER_TARGET}s each"
