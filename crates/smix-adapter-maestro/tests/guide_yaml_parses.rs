//! Every yaml block the guides show must parse with the real parser.
//!
//! The command gates pin the guides' `smix …` lines to `--help` and
//! their file paths to the filesystem — but the guides are mostly yaml,
//! and none of the 49 blocks across the reference, actions and cookbook
//! pages had ever been fed to the parser they document. The quickstart
//! walk showed how that ends: prose that agrees with itself and not
//! with the code.
//!
//! Fragments (step lists without an `appId:` header) are wrapped in the
//! minimal harness a reader would put around them; blocks that carry
//! their own header run as-is.

use smix_adapter_maestro::parse_flow_yaml;

const GUIDES: &[(&str, &str)] = &[
    (
        "02-yaml-reference",
        include_str!("../../../docs/ai-guide/02-yaml-reference.md"),
    ),
    (
        "04-actions",
        include_str!("../../../docs/ai-guide/04-actions.md"),
    ),
    (
        "08-cookbook",
        include_str!("../../../docs/ai-guide/08-cookbook.md"),
    ),
    (
        "03-selectors",
        include_str!("../../../docs/ai-guide/03-selectors.md"),
    ),
    (
        "10-ai-assertions",
        include_str!("../../../docs/ai-guide/10-ai-assertions.md"),
    ),
];

fn yaml_blocks(doc: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_block = false;
    let mut cur = String::new();
    for line in doc.lines() {
        if in_block {
            if line.trim_end() == "```" {
                out.push(std::mem::take(&mut cur));
                in_block = false;
            } else {
                cur.push_str(line);
                cur.push('\n');
            }
        } else if line.trim_end() == "```yaml" {
            in_block = true;
        }
    }
    out
}

/// A block that already carries the flow header runs as-is; a bare step
/// list gets the header any reader would give it.
fn as_flow(block: &str) -> String {
    // A real flow header always ends at a column-zero `---`; an
    // indented `appId:` (launchApp's override arg) is not a header.
    // YAML permits a trailing comment on the separator line — the
    // reference guide's flagship block annotates its `---` exactly so,
    // and requiring a bare `---` here double-wrapped that block.
    let is_separator = |l: &str| {
        l.strip_prefix("---")
            .is_some_and(|rest| rest.trim_start().is_empty() || rest.trim_start().starts_with('#'))
    };
    if block.lines().any(is_separator) {
        block.to_string()
    } else {
        format!("appId: com.example.app\n---\n{block}")
    }
}

#[test]
fn every_yaml_block_in_the_guides_parses() {
    let mut checked = 0usize;
    let mut bad: Vec<String> = Vec::new();
    for (name, doc) in GUIDES {
        for (i, block) in yaml_blocks(doc).iter().enumerate() {
            // Config-file examples (sims.json / config.yaml shapes) are
            // not flows; a block with no verb-looking `- ` entry and no
            // header is out of the parser's jurisdiction.
            if !block.contains("- ") && !block.contains("appId:") {
                continue;
            }
            checked += 1;
            // The AI-assertion verbs refuse to parse unless the tier is
            // deliberately enabled — which is itself a promise
            // 10-ai-assertions makes ("fails at parse time, before the
            // device is touched"). Its examples are written for a
            // reader who has turned it on, so the gate turns it on to
            // read them, and `the_ai_tier_refuses_when_off` covers the
            // other half.
            let ai = *name == "10-ai-assertions";
            smix_adapter_maestro::set_ai_assertions_override(Some(ai));
            let parsed = parse_flow_yaml(&as_flow(block));
            smix_adapter_maestro::set_ai_assertions_override(None);
            if let Err(e) = parsed {
                bad.push(format!("{name} block #{}:\n{block}\n  → {e}", i + 1));
            }
        }
    }
    assert!(
        checked >= 40,
        "extracted only {checked} yaml flows from the guides — the \
         extraction stopped matching and this check would pass by \
         knowing nothing"
    );
    assert!(
        bad.is_empty(),
        "{} yaml blocks in the guides do not parse:\n\n{}",
        bad.len(),
        bad.join("\n\n")
    );
}

/// The guide promises these verbs fail at parse time when the tier is
/// off, "before the device is touched". That refusal is the feature —
/// a flow must not reach a non-reproducible model call by accident —
/// so it is tested rather than assumed.
#[test]
fn the_ai_tier_refuses_when_off() {
    for verb in [
        "- assertCondition: \"a toast is visible\"\n",
        "- extractWithAI:\n    total: \"the cart total\"\n",
    ] {
        let yaml = format!("appId: com.example.app\n---\n{verb}");
        smix_adapter_maestro::set_ai_assertions_override(Some(false));
        let err = parse_flow_yaml(&yaml).expect_err("must refuse while the tier is off");
        smix_adapter_maestro::set_ai_assertions_override(None);
        let msg = format!("{err}");
        assert!(
            msg.contains("SMIX_ENABLE_AI_ASSERTIONS"),
            "the refusal must say how to turn it on: {msg}"
        );
    }
}
