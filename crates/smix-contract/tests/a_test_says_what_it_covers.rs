//! A test case saying which contract it covers, in a comment.
//!
//! The id has to live where the person editing the test will see it,
//! and the only place that works on every framework on both platforms
//! is a comment. An annotation would give compile-time checking and
//! would require the product's unit-test target to depend on smix,
//! which inverts what this tool is for.
//!
//! A comment is invisible to the compiler, so a mistyped id is caught
//! by the reconciliation rather than the build — which is why
//! `UnknownContract` exists and is an error rather than a warning.

use smix_contract::scan_claims;

#[test]
fn a_marked_line_becomes_a_claim() {
    let src = "\
@Test(\"every section is separated when nothing is hidden\")
// covers: CTR-MENU-0001
func everySectionIsSeparated() {}
";
    let claims = scan_claims(src, "MenuTests.swift", "ios");
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].contract_id, "CTR-MENU-0001");
    assert_eq!(claims[0].platform, "ios");
    // The origin names the file and the line, because a refusal that
    // says only "somewhere in this repo" sends the reader searching.
    assert_eq!(claims[0].origin, "MenuTests.swift:2");
}

#[test]
fn one_line_may_claim_several() {
    // A case covering two requirements is ordinary. Making people
    // write two lines for it makes them write none.
    let src = "// covers: CTR-0001, CTR-0002\n";
    let claims = scan_claims(src, "T.kt", "android");
    let ids: Vec<&str> = claims.iter().map(|c| c.contract_id.as_str()).collect();
    assert_eq!(ids, vec!["CTR-0001", "CTR-0002"]);
}

#[test]
fn spelling_is_forgiven_and_a_different_word_is_not() {
    // Case and spacing vary between people and formatters, so those
    // are forgiven. `coverage:` is a different word — forgiving it
    // would stop "looks like a claim" and "is a claim" being tellable
    // apart, and this crate is entirely about that distinction.
    let lenient = scan_claims("//Covers:CTR-1\n   //  COVERS:  CTR-2\n", "a", "ios");
    let ids: Vec<&str> = lenient.iter().map(|c| c.contract_id.as_str()).collect();
    assert_eq!(ids, vec!["CTR-1", "CTR-2"]);

    assert!(scan_claims("// coverage: CTR-1\n", "a", "ios").is_empty());
    assert!(scan_claims("// covered: CTR-1\n", "a", "ios").is_empty());
}

#[test]
fn a_file_with_no_marks_yields_none_and_no_error() {
    // Most source files carry no claim, and that is not a problem to
    // report.
    let src = "func ordinary() {}\nclass Thing {}\n";
    assert!(scan_claims(src, "Ordinary.swift", "ios").is_empty());
}

#[test]
fn the_same_id_twice_in_one_file_is_one_claim() {
    // Two mentions of one id in one file is one place covering it.
    // Counting them twice is the same mistake as counting two claims
    // from one platform as two platforms.
    let src = "// covers: CTR-0001\nfunc a() {}\n// covers: CTR-0001\nfunc b() {}\n";
    let claims = scan_claims(src, "T.swift", "ios");
    assert_eq!(claims.len(), 1);
    // And it names the first place, not the last.
    assert_eq!(claims[0].origin, "T.swift:1");
}

#[test]
fn an_empty_mark_claims_nothing_rather_than_something_blank() {
    // `// covers:` with nothing after it is somebody who meant to fill
    // it in. An empty contract id would reconcile against nothing and
    // read as a claim.
    assert!(scan_claims("// covers:\n", "a", "ios").is_empty());
    assert!(scan_claims("// covers:   \n", "a", "ios").is_empty());
}
