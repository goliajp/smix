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
            if let Err(e) = parse_flow_yaml(&as_flow(block)) {
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
