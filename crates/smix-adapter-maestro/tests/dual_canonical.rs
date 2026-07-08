//! Parser accepts both Maestro-canonical and smix-canonical verb names
//! for the same Step. `smix migrate` produces `tap`, `smix run` must
//! accept it. The parser normalizes smix→maestro via
//! smix_verbs::VERB_TABLE so both forms parse to the same Step IR.

use smix_adapter_maestro::{Step, parse_flow_yaml};

fn first_step(yaml: &str) -> Step {
    let flow = parse_flow_yaml(yaml).expect("parse ok");
    flow.steps.into_iter().next().expect("has step")
}

#[test]
fn tap_and_tapon_produce_same_step() {
    let maestro = "
appId: com.example
---
- tapOn: Login
";
    let smix = "
appId: com.example
---
- tap: Login
";
    match (first_step(maestro), first_step(smix)) {
        (
            Step::TapOn {
                selector: s1,
                optional: o1,
            },
            Step::TapOn {
                selector: s2,
                optional: o2,
            },
        ) => {
            assert_eq!(s1, s2);
            assert_eq!(o1, o2);
        }
        (a, b) => panic!("expected both TapOn, got {a:?} vs {b:?}"),
    }
}

#[test]
fn inputtext_and_fill_produce_same_step() {
    let maestro = "
appId: com.example
---
- inputText: hello
";
    let smix = "
appId: com.example
---
- fill: hello
";
    match (first_step(maestro), first_step(smix)) {
        (Step::InputText(a), Step::InputText(b)) => assert_eq!(a, b),
        (a, b) => panic!("expected both InputText, got {a:?} vs {b:?}"),
    }
}

#[test]
fn erasetext_and_clear_produce_same_step() {
    let maestro = "
appId: com.example
---
- eraseText: 5
";
    let smix = "
appId: com.example
---
- clear: 5
";
    let a = first_step(maestro);
    let b = first_step(smix);
    assert!(
        std::mem::discriminant(&a) == std::mem::discriminant(&b),
        "expected same variant, got {a:?} vs {b:?}"
    );
}

#[test]
fn stopapp_and_terminate_both_parse() {
    let maestro = "
appId: com.example
---
- stopApp
";
    let smix = "
appId: com.example
---
- terminate
";
    let a = first_step(maestro);
    let b = first_step(smix);
    assert!(
        std::mem::discriminant(&a) == std::mem::discriminant(&b),
        "expected same variant, got {a:?} vs {b:?}"
    );
}

#[test]
fn clearstate_and_reset_both_parse() {
    let maestro = "
appId: com.example
---
- clearState:
    appId: com.example
";
    let smix = "
appId: com.example
---
- reset:
    appId: com.example
";
    let a = first_step(maestro);
    let b = first_step(smix);
    assert!(
        std::mem::discriminant(&a) == std::mem::discriminant(&b),
        "expected same variant, got {a:?} vs {b:?}"
    );
}

#[test]
fn openlink_and_openurl_both_parse() {
    let maestro = "
appId: com.example
---
- openLink: https://example.com
";
    let smix = "
appId: com.example
---
- openUrl: https://example.com
";
    let a = first_step(maestro);
    let b = first_step(smix);
    assert!(
        std::mem::discriminant(&a) == std::mem::discriminant(&b),
        "expected same variant, got {a:?} vs {b:?}"
    );
}

#[test]
fn migrate_output_reparses() {
    // Core acceptance: migrate produces smix-canonical output; run
    // must parse the output without error.
    let maestro = "
appId: com.example
---
- tapOn: Login
- inputText: alice@example.com
- assertVisible: Dashboard
- stopApp
- clearState:
    appId: com.example
- openLink: https://foo.bar
";
    let (migrated, _report) = smix_migrate::Migrator::default()
        .migrate(maestro)
        .expect("migrate ok");
    parse_flow_yaml(&migrated).expect("smix run parses migrated output");
}

#[test]
fn assertvisible_and_expect_string_form() {
    // `expect: <string>` should fall through to assertVisible per
    // parser's expect handler.
    let maestro = "
appId: com.example
---
- assertVisible: Login
";
    let smix = "
appId: com.example
---
- expect: Login
";
    let a = first_step(maestro);
    let b = first_step(smix);
    assert!(
        std::mem::discriminant(&a) == std::mem::discriminant(&b),
        "expected both AssertVisible, got {a:?} vs {b:?}"
    );
}
