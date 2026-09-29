//! A runner's own explanation reaches the caller whole.
//!
//! The part of a runner error that says what happened is at its end. Cut
//! at 200 characters, a fill that lost its text read "… The field he" and
//! the two field values that were the evidence were gone.

use smix_runner_client::error_body;

#[test]
fn a_structured_runner_error_is_kept_whole() {
    let body = r#"{"error":"text_did_not_land","message":"input-text: typed \"run1-first\" into dev.smix.fixture:id\/fixture_input in 1 chunk(s); chunk 1 did not land after 0 retype(s) of its missing tail. The field held \"\" and holds \"un1-first\". Characters are missing from the middle, or there are characters this step did not type — neither is repaired by typing more."}"#;
    assert!(body.chars().count() > 200);
    let kept = error_body(body);
    assert_eq!(kept, body);
    assert!(kept.contains("holds \\\"un1-first\\\""));
}

#[test]
fn anything_else_is_kept_to_its_start() {
    let page = format!("<html><body>{}</body></html>", "x".repeat(500));
    assert_eq!(error_body(&page).chars().count(), 200);
    // JSON without a message of its own is not the runner explaining.
    let other = format!(r#"{{"detail":"{}"}}"#, "y".repeat(500));
    assert_eq!(error_body(&other).chars().count(), 200);
}
