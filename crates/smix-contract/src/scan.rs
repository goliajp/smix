//! Reading claims out of source, where the person editing the test sees them.

use crate::Claim;

/// The word, and only this word.
///
/// Case and surrounding space vary between people and formatters and
/// are forgiven. `coverage:` and `covered:` are not: forgiving a
/// different word would stop "looks like a claim" and "is a claim"
/// being tellable apart, and that distinction is the whole of this
/// crate.
const MARK: &str = "covers:";

/// Every claim a source file makes, in the order it makes them.
///
/// Line-oriented and language-agnostic on purpose: Swift, Kotlin,
/// Rust and anything else with `//` comments take the same path, and
/// nothing here parses a language. A claim is a statement by a person,
/// not a property the compiler can see — which is why a mistyped id is
/// caught by reconciliation and not by the build.
///
/// The same id twice in one file is one claim, named at the first
/// place it appears. Two mentions are one place covering it; counting
/// them twice is the mistake this crate refuses one level up, where
/// two claims from one platform are not two platforms.
pub fn scan_claims(source: &str, path: &str, platform: &str) -> Vec<Claim> {
    let mut out: Vec<Claim> = Vec::new();
    for (i, line) in source.lines().enumerate() {
        let Some(ids) = marked_ids(line) else {
            continue;
        };
        for id in ids {
            if out
                .iter()
                .any(|c| c.contract_id == id && c.platform == platform)
            {
                continue;
            }
            out.push(Claim {
                contract_id: id,
                platform: platform.to_string(),
                origin: format!("{path}:{}", i + 1),
            });
        }
    }
    out
}

/// The ids a line claims, if it is a claim line at all.
fn marked_ids(line: &str) -> Option<Vec<String>> {
    let trimmed = line.trim_start();
    let body = trimmed.strip_prefix("//")?.trim_start();
    // Match the mark case-insensitively without allocating a lowercase
    // copy of every line in every file.
    //
    // Byte-sliced with `get`, not `[..]`. The first version indexed
    // directly and panicked on the first comment line beginning with a
    // multi-byte character — an em dash, which this repository's own
    // prose is full of. Every `//` line in every scanned file goes
    // through here, so the one that is not a claim must cost nothing
    // and must not be able to bring the scan down.
    if !body.get(..MARK.len())?.eq_ignore_ascii_case(MARK) {
        return None;
    }
    let ids: Vec<String> = body[MARK.len()..]
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    // `// covers:` with nothing after it is somebody who meant to fill
    // it in. An empty id would reconcile against no contract and read
    // as a claim on the way past.
    if ids.is_empty() { None } else { Some(ids) }
}
