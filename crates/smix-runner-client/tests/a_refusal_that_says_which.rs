//! `ok:false` used to be one sentence for several different situations.
//!
//! A consumer met `runner /hide-keyboard answered ok:false — the action did
//! not happen` with the keyboard unmistakably on screen, and could not tell
//! it from the answer they would have got for no keyboard at all. Three
//! situations reached them identically: the dismiss strategies ran and the
//! keyboard stayed, XCUITest raised while looking, and the request context
//! was lost. What to do next differs in each — retry, restart the runner,
//! or nothing at all.
//!
//! The runner names which now. This is the client half: the name has to
//! survive `require_ok`, which used to read `ok` and discard the rest.

use smix_runner_client::RunnerTransportError;

#[tokio::test]
async fn a_named_refusal_keeps_what_the_runner_saw() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/hide-keyboard"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(
            r#"{"ok":false,"error":"keyboard_did_not_close","saw":"tried key:Return, tap-above, swipe-down; focus: input-password"}"#,
        ))
        .mount(&server)
        .await;

    let client = smix_runner_client::HttpRunnerClient::with_base(server.uri());
    let err = client
        .hide_keyboard()
        .await
        .expect_err("the runner refused");

    let said = format!("{err}");
    assert!(
        said.contains("keyboard_did_not_close"),
        "the caller cannot act on a refusal that will not say which one — {said}"
    );
    assert!(
        said.contains("swipe-down") && said.contains("input-password"),
        "and what the runner saw is the half they act on — {said}"
    );
    assert!(
        !matches!(err, RunnerTransportError::Refused { .. }),
        "the bare refusal is the shape that lost this"
    );
}

#[tokio::test]
async fn an_unnamed_refusal_still_reads_as_it_did() {
    // Routes that answer a plain `{"ok":false}` are unchanged: there is
    // nothing extra to say about them, and inventing a name would be a
    // sentence with no evidence behind it.
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/hide-keyboard"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(r#"{"ok":false}"#))
        .mount(&server)
        .await;

    let client = smix_runner_client::HttpRunnerClient::with_base(server.uri());
    let err = client
        .hide_keyboard()
        .await
        .expect_err("the runner refused");
    assert!(
        matches!(err, RunnerTransportError::Refused { .. }),
        "a body with nothing extra stays the shape it was"
    );
}
