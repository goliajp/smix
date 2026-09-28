//! `smix_use` has the same short-circuit `runner up` had: `/health`
//! answers, so the tool reports it is already driving and stops.
//!
//! Source-level, like the other assertions in this crate: `main.rs` is a
//! binary, so no test can call the tool. The shape being asserted is
//! structural anyway — the defect was that one question stood in for
//! another, and that is visible in which questions the body asks.

use std::fs;

const MAIN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/main.rs");

fn use_body() -> String {
    let src = fs::read_to_string(MAIN).expect("main.rs");
    let after = src
        .split("async fn smix_use")
        .nth(1)
        .expect("smix_use is a tool on this server");
    after
        .split("async fn smix_")
        .next()
        .expect("the tool's own body")
        .to_string()
}

#[test]
fn the_short_circuit_asks_whether_the_session_works() {
    let body = use_body();
    assert!(
        body.contains("health_ok"),
        "smix_use no longer reads /health at all — this test is reading air, \
         and the rule it carries needs rewriting rather than deleting"
    );
    assert!(
        body.contains("probe_session"),
        "smix_use decides it is already driving from /health alone. /health is \
         a closure over a boot date: it says the server answers, and cannot see \
         that the app binding died underneath it"
    );
}

#[test]
fn a_dead_session_is_told_what_to_run() {
    let body = use_body();
    assert!(
        body.contains("smix runner cycle"),
        "an agent that cannot drive has to be handed the command that fixes \
         it — §9 #5. The body never names one"
    );
}
