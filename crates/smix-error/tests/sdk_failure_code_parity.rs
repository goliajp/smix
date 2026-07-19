//! One vocabulary, four languages.
//!
//! `FailureCode` existed four times over — once here, once in each SDK —
//! and the three SDK copies had drifted onto a camelCase six-value
//! vocabulary whose intersection with this crate's wire strings was
//! empty, all three still carrying a comment claiming to mirror Rust. No
//! SDK decoded a Rust-produced failure yet, so nothing crashed; the
//! contract was simply a lie waiting for the first decoder.
//!
//! This reads each SDK's declaration out of its source and refuses to let
//! them differ. Rust is the source: the expected strings come from
//! serializing every variant, never from a list written by hand here.

use smix_error::FailureCode;
use std::collections::BTreeSet;

/// Every variant, listed once.
///
/// The exhaustive match below is the mechanical link: adding a variant to
/// `FailureCode` stops this file compiling until the variant is added
/// here too, and every assertion in this test file is derived from this
/// list by serializing it — no wire string is spelled out by hand.
fn all_variants() -> Vec<FailureCode> {
    let all = vec![
        FailureCode::ElementNotFound,
        FailureCode::NotVisible,
        FailureCode::NotEnabled,
        FailureCode::Ambiguous,
        FailureCode::Timeout,
        FailureCode::AssertionFailed,
        FailureCode::AppNotRunning,
        FailureCode::SimulatorNotBooted,
        FailureCode::DriverError,
    ];
    for code in &all {
        // Exhaustive on purpose — a new variant must fail to compile.
        match code {
            FailureCode::ElementNotFound
            | FailureCode::NotVisible
            | FailureCode::NotEnabled
            | FailureCode::Ambiguous
            | FailureCode::Timeout
            | FailureCode::AssertionFailed
            | FailureCode::AppNotRunning
            | FailureCode::SimulatorNotBooted
            | FailureCode::DriverError => {}
        }
    }
    all
}

/// The wire strings, straight off serde.
fn rust_wire_codes() -> BTreeSet<String> {
    let codes: BTreeSet<String> = all_variants()
        .into_iter()
        .map(|c| {
            serde_json::to_value(c)
                .expect("FailureCode serializes")
                .as_str()
                .expect("FailureCode serializes as a JSON string")
                .to_owned()
        })
        .collect();
    assert_eq!(
        codes.len(),
        all_variants().len(),
        "two variants serialize to the same wire string"
    );
    codes
}

/// Slice the block between the line containing `opener` and the next line
/// that is exactly `closer` at column 0 — enough structure for the three
/// one-block declarations this file reads.
fn block<'a>(src: &'a str, opener: &str, closer: &str, what: &str) -> &'a str {
    let start = src
        .find(opener)
        .unwrap_or_else(|| panic!("{what}: no declaration matching `{opener}` — it was renamed or restructured, and this check would pass by knowing nothing"));
    let rest = &src[start..];
    let end = rest.find(closer).unwrap_or_else(|| {
        panic!("{what}: declaration starting at `{opener}` never closed with `{closer}`")
    });
    &rest[..end]
}

/// Every `"..."`-quoted string in `s`, using `quote` as the delimiter.
fn quoted(s: &str, quote: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut it = s.split(quote);
    // Odd indices are the insides of the quotes.
    let _ = it.next();
    while let Some(inside) = it.next() {
        out.push(inside.to_owned());
        if it.next().is_none() {
            break;
        }
    }
    out
}

/// Wire strings look like `ELEMENT_NOT_FOUND` — filtering on that shape
/// keeps unrelated quoted text (JSON keys, doc prose) out of the set
/// without hiding a genuine mismatch, since anything SCREAMING_SNAKE
/// inside these blocks *is* a declared code.
fn screaming_snake(candidates: Vec<String>) -> BTreeSet<String> {
    candidates
        .into_iter()
        .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_uppercase() || c == '_'))
        .collect()
}

fn assert_parity(declared: BTreeSet<String>, sdk: &str) {
    let expected = rust_wire_codes();
    assert!(
        !declared.is_empty(),
        "{sdk}: read no codes out of the declaration — its shape changed and \
         this check would pass by knowing nothing"
    );
    assert_eq!(
        declared.len(),
        expected.len(),
        "{sdk}: read {} codes, expected {} — got {declared:?}",
        declared.len(),
        expected.len()
    );
    assert_eq!(
        declared, expected,
        "{sdk} declares a different failure vocabulary than Rust"
    );
}

#[test]
fn swift_declares_the_rust_vocabulary() {
    let src = include_str!("../../../swift-bridge/Sources/SmixSDK/ExpectationFailure.swift");
    let decl = block(
        src,
        "public enum FailureCode: String",
        "\n}",
        "Swift SmixSDK.FailureCode",
    );
    assert_parity(screaming_snake(quoted(decl, '"')), "Swift");
}

#[test]
fn kotlin_declares_the_rust_vocabulary() {
    let src = include_str!(
        "../../../android-runner/sdk/src/main/kotlin/dev/smix/sdk/ExpectationFailure.kt"
    );
    let decl = block(
        src,
        "enum class FailureCode {",
        "\n}",
        "Kotlin dev.smix.sdk.FailureCode",
    );
    assert_parity(screaming_snake(quoted(decl, '"')), "Kotlin");
}

#[test]
fn typescript_declares_the_rust_vocabulary() {
    let src = include_str!("../../../npm/smix-rn/src/ExpectationFailure.ts");
    // The union type ends at the blank line before the const.
    let decl = block(
        src,
        "export type FailureCode =",
        "\n\n",
        "TypeScript FailureCode union",
    );
    assert_parity(screaming_snake(quoted(decl, '\'')), "TypeScript union");

    // The runtime array is what consumers iterate, so it is checked too —
    // a union and an array that disagree is the same drift one level down.
    // Anchored past the `readonly FailureCode[]` annotation, whose own
    // `]` would otherwise close the block before the first code.
    let array = block(
        src,
        "export const FAILURE_CODES: readonly FailureCode[] = [",
        "] as const",
        "TypeScript FAILURE_CODES",
    );
    assert_parity(
        screaming_snake(quoted(array, '\'')),
        "TypeScript FAILURE_CODES",
    );
}

/// The extractors must be able to fail. If a declaration is emptied out,
/// the check has to notice rather than agree with nothing.
#[test]
fn an_emptied_declaration_is_caught() {
    let empty = screaming_snake(quoted("public enum FailureCode: String {\n}", '"'));
    assert!(empty.is_empty(), "fixture has no codes to find");
    let caught = std::panic::catch_unwind(|| assert_parity(empty, "fixture"));
    assert!(
        caught.is_err(),
        "a declaration with no codes in it must fail the parity check"
    );
}

/// The three SDK READMEs, which document the vocabulary for readers.
///
/// The source-level checks above caught the drift in code; the pages
/// that TELL people which codes to catch went on teaching the retired
/// camelCase six — `.notFound`, `.notInteractable`, `.wrongState`,
/// `.unknown` — long after the sources were corrected. A reader who
/// copies a code from a README gets a comparison that is never true.
const SDK_READMES: &[(&str, &str)] = &[
    (
        "swift-bridge/README.md",
        include_str!("../../../swift-bridge/README.md"),
    ),
    (
        "android-runner/sdk/README.md",
        include_str!("../../../android-runner/sdk/README.md"),
    ),
    (
        "npm/smix-rn/README.md",
        include_str!("../../../npm/smix-rn/README.md"),
    ),
];

/// Codes the retired vocabulary used that the real one never had. A
/// README naming any of them is teaching a value that cannot occur.
const RETIRED_SPELLINGS: &[&str] = &[
    "notFound",
    "NOT_FOUND",
    "notInteractable",
    "NOT_INTERACTABLE",
    "wrongState",
    "WRONG_STATE",
];

#[test]
fn no_sdk_readme_documents_a_retired_failure_code() {
    let mut found: Vec<String> = Vec::new();
    for (name, text) in SDK_READMES {
        for (lineno, line) in text.lines().enumerate() {
            for retired in RETIRED_SPELLINGS {
                // `ELEMENT_NOT_FOUND` legitimately contains `NOT_FOUND`.
                if line.contains(retired) && !line.contains("ELEMENT_NOT_FOUND") {
                    found.push(format!("{name}:{}: `{retired}`", lineno + 1));
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "SDK READMEs document failure codes that do not exist:\n  {}\n\
         The real vocabulary is {:?}",
        found.join("\n  "),
        all_variants()
            .iter()
            .map(|c| serde_json::to_value(c).expect("serializes"))
            .collect::<Vec<_>>()
    );
}

/// Every SCREAMING_SNAKE token a README presents as a failure code must
/// be one Rust actually emits.
#[test]
fn every_failure_code_an_sdk_readme_names_is_real() {
    let real: BTreeSet<String> = all_variants()
        .iter()
        .map(|c| {
            serde_json::to_value(c)
                .expect("serializes")
                .as_str()
                .expect("string")
                .to_string()
        })
        .collect();

    let mut checked = 0usize;
    let mut bogus: Vec<String> = Vec::new();
    for (name, text) in SDK_READMES {
        for (lineno, line) in text.lines().enumerate() {
            // Only lines presenting the code field, so prose about e.g.
            // HTTP verbs is not mistaken for a vocabulary claim.
            if !line.contains("code") {
                continue;
            }
            for token in line.split(|c: char| !(c.is_ascii_uppercase() || c == '_')) {
                if token.len() < 5 || !token.contains('_') {
                    continue;
                }
                checked += 1;
                if !real.contains(token) {
                    bogus.push(format!("{name}:{}: `{token}`", lineno + 1));
                }
            }
        }
    }
    assert!(
        checked >= 3,
        "found only {checked} code-shaped tokens across the SDK READMEs — \
         the extraction stopped matching and this check would pass by \
         knowing nothing"
    );
    assert!(
        bogus.is_empty(),
        "SDK READMEs name failure codes Rust never emits:\n  {}",
        bogus.join("\n  ")
    );
}

/// The errors guide must document every code, not merely avoid naming
/// fake ones. A variant added to `FailureCode` that no guide mentions
/// is a failure mode users meet with no page to read.
#[test]
fn the_errors_guide_documents_every_variant() {
    const ERRORS_GUIDE: &str = include_str!("../../../docs/ai-guide/07-errors.md");
    let undocumented: Vec<String> = all_variants()
        .iter()
        .map(|c| {
            serde_json::to_value(c)
                .expect("serializes")
                .as_str()
                .expect("string")
                .to_string()
        })
        .filter(|wire| !ERRORS_GUIDE.contains(wire.as_str()))
        .collect();
    assert!(
        undocumented.is_empty(),
        "docs/ai-guide/07-errors.md never mentions {undocumented:?} — a \
         code a user can receive with no page explaining it"
    );
}
