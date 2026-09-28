//! Every tool that takes a selector answers for the forms that cannot be
//! resolved, and the schema does not promise anything it does not do.
//!
//! `Point` and `Fallback` are dispatched by the caller and never reach a
//! match in the tree — the resolver returns false for both by design. A
//! tool that takes one and calls `App::find` or `App::tap` does not fail:
//! it reports that a place the touch could plainly reach is not there.
//! Silence is the failure mode, so this is a source-level assertion.
//!
//! The other half matters more here than anywhere else in the codebase.
//! The doc comments on `SelectorParams` are serialized into the tool
//! schema and are the *only* documentation the calling agent ever sees.
//! A sentence there that the code does not honour is not a stale comment
//! — it is an instruction to an agent that will act on it.

use std::fs;

const MAIN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/main.rs");
const PARAMS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/selector_params.rs");

fn tool_bodies() -> Vec<(String, String)> {
    let src = fs::read_to_string(MAIN).expect("main.rs");
    let mut out = Vec::new();
    for chunk in src.split("async fn smix_").skip(1) {
        let name = chunk.split('(').next().unwrap_or("?").to_string();
        // Up to the next tool, so a body is judged on its own.
        let body = chunk
            .split("async fn smix_")
            .next()
            .unwrap_or("")
            .to_string();
        // By what the body does, not by its parameter type: `smix_fill`
        // and `smix_scroll` nest a `SelectorParams` as `target` rather
        // than taking one directly, and looking for the type name missed
        // exactly the two tools that turned out not to check.
        if body.contains("to_selector()") {
            out.push((name, body));
        }
    }
    out
}

/// Every tool that takes a selector says what it does with a point:
/// dispatches it, or refuses it. Handing it to the resolver is the third
/// option and the wrong one.
#[test]
fn every_selector_tool_answers_for_a_point() {
    let tools = tool_bodies();
    assert!(
        tools.len() >= 6,
        "only {} tool(s) take a selector — this test is reading air",
        tools.len()
    );
    for (name, body) in &tools {
        assert!(
            body.contains("point_of"),
            "smix_{name} takes a selector and never asks whether it is a point. \
             It will hand one to the resolver, which answers 'not found' about a \
             place the touch could have reached"
        );
    }
}

/// The schema's promises are kept. `point`'s doc tells the agent that
/// find, assert and fill refuse a point; three of those four did.
#[test]
fn the_schema_promises_only_what_the_tools_do() {
    let params = fs::read_to_string(PARAMS).expect("selector_params.rs");
    let doc: String = params
        .lines()
        .filter(|l| l.trim_start().starts_with("///"))
        .collect::<Vec<_>>()
        .join(" ");
    let promised: Vec<&str> = ["found", "asserted", "filled"]
        .into_iter()
        .filter(|verb| doc.contains(verb))
        .collect();
    assert!(
        !promised.is_empty(),
        "the point doc names no refusing tool — this test is reading air"
    );

    let tools = tool_bodies();
    for (verb, tool) in [
        ("found", "find"),
        ("asserted", "assert_visible"),
        ("filled", "fill"),
    ] {
        if !promised.contains(&verb) {
            continue;
        }
        let (_, body) = tools
            .iter()
            .find(|(n, _)| n == tool)
            .unwrap_or_else(|| panic!("smix_{tool} exists"));
        assert!(
            body.contains("point_of"),
            "the schema tells the agent a point cannot be {verb}, and smix_{tool} \
             never checks. That doc is the agent's only documentation; a promise \
             in it that the code does not keep is an instruction to act wrongly"
        );
    }
}

/// Same rule, same reason, for chains. `Fallback` is dispatched by the
/// caller too — the resolver returns false for it — so a tool that hands
/// one to `App::find` gets a single no where the caller asked for several
/// tries, and cannot tell that from the thing being absent.
#[test]
fn every_selector_tool_answers_for_a_chain() {
    let tools = tool_bodies();
    assert!(
        tools.len() >= 6,
        "only {} tool(s) take a selector — this test is reading air",
        tools.len()
    );
    for (name, body) in &tools {
        assert!(
            body.contains("chain_of"),
            "smix_{name} takes a selector and never asks whether it is a chain.              It will hand one to the resolver, which says no once instead of              trying each layer"
        );
    }
}

/// A chain is only worth having if some tool walks it. If every tool
/// refused, the field would be documentation for a capability nobody has.
#[test]
fn at_least_one_tool_walks_the_chain() {
    let walkers: Vec<String> = tool_bodies()
        .into_iter()
        .filter(|(_, b)| b.contains("first_visible_layer") || b.contains("for (i, layer)"))
        .map(|(n, _)| n)
        .collect();
    assert!(
        walkers.len() >= 3,
        "only {walkers:?} walk a chain — a field every tool refuses is a          promise in the schema with nothing behind it"
    );
}
