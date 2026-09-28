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
fn a_comment_that_is_not_a_claim_cannot_bring_the_scan_down() {
    // Every `//` line in every scanned file goes through the same
    // check, so the overwhelming majority of them are not claims. The
    // first version sliced the line by byte index to compare against
    // the mark and panicked on the first one beginning with a
    // multi-byte character — an em dash, of which this repository's
    // prose is full. It was found by a fixture written to look like
    // real source rather than like a test input, which is the whole
    // reason to write fixtures that way.
    for line in [
        // The bytes are the subject here, not the language. A CJK line
        // would do as well and the repository's hygiene scan cannot tell
        // a comment from a fixture that looks like one — which is the
        // right way for that scan to be wrong.
        "// — a dash begins this line\n",
        "// ¡mult\u{00ed}byte, en el primer car\u{00e1}cter!\n",
        "//\n",
        "//c\n",
        "// cov\n",
        "// ☂\n",
    ] {
        let _ = scan_claims(line, "a", "ios");
    }
    // And a real claim still reads after all that.
    assert_eq!(scan_claims("// covers: CTR-1\n", "a", "ios").len(), 1);
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

// ---- the two platforms, in the shapes they really have ----------------

fn platform_file(name: &str, platform: &str) -> Vec<smix_contract::Claim> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("corpus")
        .join("platforms")
        .join(name);
    let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
    scan_claims(&src, name, platform)
}

#[test]
fn a_swift_suite_declares_what_it_covers() {
    let claims = platform_file("MenuEntriesTests.swift", "ios");
    let ids: Vec<&str> = claims.iter().map(|c| c.contract_id.as_str()).collect();
    assert_eq!(
        ids,
        vec![
            "CTR-MENU-0001",
            "CTR-MENU-0002",
            "CTR-MENU-0003",
            "CTR-MENU-0004",
            // See the test below: 0005 belongs to a case that is
            // commented out, and the scanner counts it.
            "CTR-MENU-0005",
        ]
    );
}

#[test]
fn a_kotlin_suite_declares_the_same_requirements() {
    // The same wording on both sides, which is the observation this
    // layer exists for. The third claim here is written without spaces
    // and in mixed case, as a formatter or a hurried person leaves it.
    let claims = platform_file("MenuEntriesTest.kt", "android");
    let ids: Vec<&str> = claims.iter().map(|c| c.contract_id.as_str()).collect();
    assert_eq!(ids, vec!["CTR-MENU-0001", "CTR-MENU-0002", "CTR-MENU-0003"]);
}

#[test]
fn a_commented_out_case_still_claims_and_that_is_a_finding() {
    // Written as an assertion before it was decided, to see what the
    // scanner does rather than to confirm what I assumed.
    //
    // It counts. A claim line is an ordinary comment and so is the
    // case beneath it, and nothing here parses a language well enough
    // to tell "commented-out code" from "prose".
    //
    // Left counting, deliberately. A claim is a STATEMENT that a suite
    // means to cover a requirement — it never promised the test runs,
    // passes, or is any good, and this crate says so in as many words.
    // A scanner that tried to tell a disabled test from an enabled one
    // would be doing the thing this layer refuses: turning a
    // declaration into a verification.
    //
    // What catches the case that matters — a requirement whose only
    // coverage is switched off — is not this. It is the test suite
    // going red, or a coverage tool, both of which watch what runs.
    let claims = platform_file("MenuEntriesTests.swift", "ios");
    let commented = claims
        .iter()
        .find(|c| c.contract_id == "CTR-MENU-0005")
        .expect("the claim above a commented-out case is still read");
    assert!(commented.origin.starts_with("MenuEntriesTests.swift:"));
}

#[test]
fn the_two_platforms_reconcile_into_the_three_sets() {
    // The point of the whole step: claims scanned out of source and
    // claims written by hand are the same thing in two notations, not
    // two sets of books.
    use smix_contract::{parse_contracts, reconcile};

    let contracts = parse_contracts(
        "\
- id: CTR-MENU-0001
  statement: Every section is separated from the next when none are hidden
- id: CTR-MENU-0002
  statement: Hiding part of a section leaves that section's separator in place
- id: CTR-MENU-0003
  statement: A middle section that is entirely hidden takes its separator with it
- id: CTR-MENU-0004
  statement: No separator leads the list when the first section is hidden
- id: CTR-MENU-0006
  statement: A single surviving section stands alone with no separators
",
        "fixture",
    )
    .expect("contracts parse");

    let mut claims = platform_file("MenuEntriesTests.swift", "ios");
    claims.extend(platform_file("MenuEntriesTest.kt", "android"));
    // 0005 is claimed by the Swift file and has no contract here, which
    // is the mistyped-id case: it must be refused, not dropped.
    let err = reconcile(&contracts, &claims, &["ios", "android"])
        .expect_err("a claim on an id no contract carries is refused");
    assert!(err.to_string().contains("CTR-MENU-0005"), "said: {err}");

    // With that one out, the three sets come out as they should.
    claims.retain(|c| c.contract_id != "CTR-MENU-0005");
    let r = reconcile(&contracts, &claims, &["ios", "android"]).expect("reconciles");

    let both: Vec<&str> = r.fully_claimed.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(
        both,
        vec!["CTR-MENU-0001", "CTR-MENU-0002", "CTR-MENU-0003"]
    );

    let partial: Vec<(&str, &str)> = r
        .partially_claimed
        .iter()
        .map(|p| (p.contract.id.as_str(), p.missing[0].as_str()))
        .collect();
    assert_eq!(partial, vec![("CTR-MENU-0004", "android")]);

    let none: Vec<&str> = r.unclaimed.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(none, vec!["CTR-MENU-0006"]);
}

// ---- both notations at once -------------------------------------------

#[test]
fn a_contract_claimed_in_source_and_by_hand_is_claimed_once() {
    // The two notations will coexist: a hand-written claim file is how
    // a team starts, and comments in the source is where it ends up.
    // While both are present the same platform may say the same thing
    // twice, and two sayings are not two platforms — the same
    // arithmetic the reconciler already refuses one level up.
    use smix_contract::{parse_claims, parse_contracts, reconcile};

    let contracts = parse_contracts(
        "- id: CTR-0001\n  statement: The app owes one thing\n",
        "fixture",
    )
    .unwrap();

    let mut claims = scan_claims("// covers: CTR-0001\n", "T.swift", "ios");
    claims.extend(parse_claims("- contract: CTR-0001\n  platform: ios\n", "claims.yaml").unwrap());
    assert_eq!(claims.len(), 2, "two notations, two claims on the way in");

    let r = reconcile(&contracts, &claims, &["ios", "android"]).expect("reconciles");
    let partial = &r.partially_claimed[0];
    assert_eq!(
        partial.claimed_by,
        vec!["ios"],
        "one platform said it twice; that is still one platform"
    );
    assert_eq!(partial.missing, vec!["android"]);
}

#[test]
fn the_two_notations_disagree_loudly_rather_than_quietly() {
    // A comment claiming an id the contract file does not carry is the
    // mistyped-id case, and it must be refused whichever notation it
    // came from. Silently dropping the source-scanned one would be
    // worse than dropping a hand-written one: nobody edits a claim file
    // by accident, and everybody edits source.
    use smix_contract::{parse_contracts, reconcile};

    let contracts = parse_contracts(
        "- id: CTR-0001\n  statement: The app owes one thing\n",
        "fixture",
    )
    .unwrap();
    let claims = scan_claims("// covers: CTR-0002\n", "T.kt", "android");
    let err = reconcile(&contracts, &claims, &["ios", "android"]).expect_err("refused");
    let said = err.to_string();
    assert!(said.contains("CTR-0002"), "said: {said}");
    // And it names the line, because the point of scanning source is
    // that the reader can go straight there.
    assert!(said.contains("T.kt:1"), "said: {said}");
}
