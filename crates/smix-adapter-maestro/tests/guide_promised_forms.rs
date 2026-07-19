//! Verb forms the guides documented that the parser never learned.
//!
//! The guide-yaml gate's first run found the reference, actions and
//! cookbook pages teaching three spellings the parser rejects:
//! `inputText:` with a target selector (the SDK's `fill(selector)` has
//! existed the whole time — only the yaml wiring was missing),
//! `openLink:` in `link:` mapping form, and `webViewEval` with the
//! capital V three of four docs use. Docs that agree with each other
//! and not with the parser — the same species the quickstart walk
//! found in the CLI surface.

use smix_adapter_maestro::parse_flow_yaml;

fn flow(body: &str) -> String {
    format!("appId: com.example.app\n---\n{body}")
}

#[test]
fn input_text_takes_a_target_selector() {
    let parsed = parse_flow_yaml(&flow(
        "- inputText:\n    id: \"form-email-input\"\n    text: \"alice@example.com\"\n",
    ))
    .expect("the targeted form both guides document must parse");
    let debug = format!("{:?}", parsed.steps);
    assert!(
        debug.contains("form-email-input") && debug.contains("alice@example.com"),
        "selector and text must both survive parsing: {debug}"
    );
}

#[test]
fn input_text_scalar_form_still_works() {
    parse_flow_yaml(&flow("- inputText: \"hello\"\n")).expect("scalar form unchanged");
}

#[test]
fn input_text_mapping_without_text_is_loud() {
    let err = parse_flow_yaml(&flow("- inputText:\n    id: \"x\"\n"))
        .expect_err("a target with nothing to type is a mistake, not empty input");
    assert!(
        format!("{err}").contains("text"),
        "must name the missing key: {err}"
    );
}

#[test]
fn open_link_accepts_the_link_mapping_form() {
    let parsed = parse_flow_yaml(&flow("- openLink:\n    link: \"https://example.com\"\n"))
        .expect("the mapping form three guides document must parse");
    assert!(format!("{:?}", parsed.steps).contains("https://example.com"));
}

#[test]
fn open_link_browser_key_is_a_loud_unsupported_error() {
    // maestro's `browser:`/`autoVerify:` options are not implemented;
    // swallowing them would silently change what the flow does.
    let err = parse_flow_yaml(&flow(
        "- openLink:\n    link: \"https://example.com\"\n    browser: true\n",
    ))
    .expect_err("an option the runtime ignores must refuse to parse");
    assert!(
        format!("{err}").contains("browser"),
        "must name the unsupported key: {err}"
    );
}

#[test]
fn webview_eval_capital_v_spelling_parses() {
    parse_flow_yaml(&flow("- webViewEval: |\n    document.title\n"))
        .expect("the spelling three of four guides use must parse");
}

#[test]
fn a_doc_separator_with_trailing_comment_is_still_a_separator() {
    // `---  # comment` is a legal YAML document separator; the
    // reference guide's flagship header example annotates it that way.
    parse_flow_yaml("appId: com.example.app\n---   # header ends here\n- stopApp\n")
        .expect("YAML allows a comment after ---");
}

#[test]
fn the_reference_guides_flagship_header_block_parses() {
    // The exact first block of 02-yaml-reference.md, comments included.
    let block = concat!(
        "appId: com.example.app             # iOS bundle id OR Android package\n",
        "# OR for cross-platform:\n",
        "app: myapp                         # logical key resolved via --apps-config apps.yaml\n",
        "---                                # YAML doc separator (header ends, flow begins)\n",
        "- launchApp:\n",
        "    clearState: true\n",
        "- assertVisible: \"Hello\"\n",
        "- tapOn: \"Submit\"\n",
    );
    parse_flow_yaml(block).expect("the guide's own header example must parse");
}

#[test]
fn bare_kill_app_means_the_current_app() {
    // Both guides teach `- killApp   # force-quit current app`; the
    // parser accepted only the named forms.
    let parsed = parse_flow_yaml(&flow("- killApp\n")).expect("bare form must parse");
    assert!(format!("{:?}", parsed.steps).contains("KillApp"));
}

#[test]
fn bare_clear_state_means_the_current_app() {
    // `- clearState   # wipe state without restart` — same promise the
    // guides make for bare killApp, same parser gap.
    let parsed = parse_flow_yaml(&flow("- clearState\n")).expect("bare form must parse");
    assert!(format!("{:?}", parsed.steps).contains("ClearState"));
}
