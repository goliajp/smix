//! What the codemod emits has to be something the parser accepts.
//!
//! The verb-table gate in `smix-adapter-maestro` checks membership: every
//! verb the parser dispatches has a row. This checks the other half, which
//! is the promise a migration tool actually makes — run your maestro flow
//! through it and the result still runs.
//!
//! It was not being kept. `doubleTapOn: Item` parses; `smix migrate` turns
//! it into `doubleTap: Item`, which does not. Same for `longPressOn`. Those
//! are maestro's own spellings, so the codemod was breaking exactly the
//! flows it exists to carry across.

use smix_migrate::Migrator;

/// A one-step flow using `verb`, with an argument shaped the way the table
/// says the verb takes one.
fn flow_using(verb: &str, arg_shape: &str) -> String {
    let step = match arg_shape {
        "None" => format!("- {verb}"),
        "Selector" | "String" => format!("- {verb}: Item"),
        "Bool" => format!("- {verb}: true"),
        "Int" => format!("- {verb}: 1"),
        _ => format!("- {verb}: Item"),
    };
    format!("appId: com.test.roundtrip\n---\n{step}\n")
}

fn parses(yaml: &str) -> bool {
    smix_adapter_maestro::parse_flow_yaml(yaml).is_ok()
}

/// The verbs whose canonical maestro spelling the codemod must carry over.
///
/// Not the whole table: a row whose `maestro_name` is a smix invention has
/// no maestro flow to migrate, and rows taking structured arguments need a
/// body this fixture does not build. Both are covered by the parser's own
/// tests; what is covered here is the promise to maestro users.
const MAESTRO_SPELLINGS: &[&str] = &[
    "tapOn",
    "doubleTapOn",
    "longPressOn",
    "inputText",
    "eraseText",
    "assertVisible",
    "assertNotVisible",
    "openLink",
    "stopApp",
    "killApp",
    "hideKeyboard",
    "back",
];

/// `back` is written bare in maestro. Everything else in the list above takes
/// a target, and the fixture gives it one.
const BARE_SPELLINGS: &[&str] = &["back"];

#[test]
fn every_maestro_spelling_survives_the_codemod() {
    let migrator = Migrator::default();
    let mut broken = Vec::new();

    for verb in MAESTRO_SPELLINGS {
        let before = if BARE_SPELLINGS.contains(verb) {
            flow_using(verb, "None")
        } else {
            flow_using(verb, "Selector")
        };
        if !parses(&before) {
            // Not the codemod's fault, but the fixture would prove nothing.
            continue;
        }
        let (after, _report) = migrator.migrate(&before).expect("codemod ran");
        if !parses(&after) {
            let emitted = after
                .lines()
                .last()
                .unwrap_or_default()
                .trim()
                .to_string();
            broken.push(format!("{verb} → {emitted}"));
        }
    }

    assert!(
        broken.is_empty(),
        "the codemod rewrote parseable maestro flows into unparseable ones:\n  {}",
        broken.join("\n  ")
    );
}

/// The codemod's fixtures record what it should emit, and were checked only
/// as text — so `expected-smix.yaml` sat there for releases saying the right
/// answer was `- pressKey` with nothing to press, which the parser rejects.
/// A fixture that cannot run is not an expectation, it is a typo with a test
/// around it.
#[test]
fn the_codemod_fixtures_expect_yaml_that_runs() {
    for (name, yaml) in [
        (
            "expected-smix.yaml",
            include_str!("../../smix-migrate/tests/fixtures/expected-smix.yaml"),
        ),
        (
            "expected-smix-with-comments.yaml",
            include_str!("../../smix-migrate/tests/fixtures/expected-smix-with-comments.yaml"),
        ),
    ] {
        assert!(
            parses(yaml),
            "{name} is the codemod's stated output and the parser rejects it"
        );
    }
}
