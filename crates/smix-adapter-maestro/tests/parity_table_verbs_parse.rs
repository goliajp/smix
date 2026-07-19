//! Anything the parity page calls a verb must parse as one.
//!
//! The page's own opening line is "what each smix YAML verb does", and
//! its tables' first column is headed `verb` — so every row is a claim
//! that a flow may write that word. Six rows could not: `tapById`,
//! `tapAtCoord`, `swipeAtCoord` and `findTextByOcr` are capabilities
//! reached through other verbs, and `ocrText` / `anchorRelative` are
//! selector forms. A flow writing any of them got `unsupported
//! command` from the very page that lists smix's coverage.
//!
//! The verb table gate pins the parser to `VERB_TABLE`; this pins the
//! page a reader plans from to the parser.

use smix_adapter_maestro::parse_flow_yaml;

const PARITY: &str = include_str!("../../../docs/ai-guide/verb-parity.md");

/// Rows under a table whose first column is headed `verb`. A table
/// headed anything else (`capability`, `form`) is describing something
/// that is deliberately not a verb, and saying so.
fn verbs_claimed(doc: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_verb_table = false;
    for line in doc.lines() {
        let t = line.trim();
        if t.starts_with('|') {
            let first = t.trim_matches('|').split('|').next().unwrap_or("").trim();
            if first.eq_ignore_ascii_case("verb") {
                in_verb_table = true;
                continue;
            }
            if first.starts_with("---") {
                continue;
            }
            if !in_verb_table {
                continue;
            }
            // A row's first cell may offer spellings: `a` / `b` / `c`.
            for spelling in first.split('/') {
                let name = spelling.trim().trim_matches('`').trim();
                if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    out.push(name.to_string());
                }
            }
        } else if !t.is_empty() {
            // Any non-table line ends the current table.
            in_verb_table = false;
        }
    }
    out
}

#[test]
fn every_verb_the_parity_page_lists_parses_as_a_verb() {
    let claimed = verbs_claimed(PARITY);
    assert!(
        claimed.len() >= 30,
        "extracted only {} verbs from the parity page — the extraction \
         stopped matching and this check would pass by knowing nothing",
        claimed.len()
    );

    let mut unsupported: Vec<String> = Vec::new();
    for verb in &claimed {
        // The argument shape differs per verb and is not the subject
        // here; `unsupported command` is the one error that means "this
        // word is not a verb at all".
        let probes = [
            format!("appId: com.example.app\n---\n- {verb}\n"),
            format!("appId: com.example.app\n---\n- {verb}: \"x\"\n"),
            format!("appId: com.example.app\n---\n- {verb}: {{ id: \"x\" }}\n"),
        ];
        let recognised = probes.iter().any(|yaml| match parse_flow_yaml(yaml) {
            Ok(_) => true,
            Err(e) => !format!("{e}").contains("unsupported command"),
        });
        if !recognised {
            unsupported.push(verb.clone());
        }
    }

    assert!(
        unsupported.is_empty(),
        "the parity page lists these as verbs, and the parser rejects \
         them as unsupported commands:\n  {}",
        unsupported.join("\n  ")
    );
}
