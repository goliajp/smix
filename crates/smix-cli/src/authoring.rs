//! Authoring tier subcommands.
//!
//! User-facing surface `smix authoring <action>` for composing
//! yaml against a live sim. Backed by the runner's a11y tree + tap
//! synthesis primitives, exposed via smix-sdk.
//!
//! # Actions
//!
//! - `smix authoring suggest '<partial-selector>'` — given a partial
//!   selector spec (`id: qa-*`, `text: /Sign.*/`), lists candidate
//!   selectors that identify matching elements in the live sim state.
//! - `smix authoring tap-record` — enters an interactive recorder;
//!   user taps a UI element, smix prints all selector shapes that
//!   identify it.
//! - `smix authoring diff-tree <baseline.json>` — semantic diff of
//!   current sim's a11y tree vs a stored baseline. Reports
//!   missing / renamed / re-parented nodes for visual gate use.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::CliError;
use crate::act;
use smix_screen::A11yNode;

/// Discovered candidate for a selector suggestion.
#[derive(Debug, Clone)]
pub struct SelectorCandidate {
    /// Selector spec ready to paste into yaml (`id: <>`, `text: "<>"`).
    pub spec: String,
    /// Human-readable role or type of the matched element.
    pub role: String,
    /// Bounds x, y, w, h.
    pub bounds: (f64, f64, f64, f64),
}

/// Enumerate candidate selectors matching a partial spec.
pub fn suggest_selectors(tree: &A11yNode, partial: &str) -> Vec<SelectorCandidate> {
    let mut out = Vec::new();
    let (field, pattern) = parse_partial(partial);
    walk_tree(tree, &mut |node| {
        if matches_partial(node, field, pattern) {
            let role_name = node
                .role
                .as_ref()
                .map(|r| format!("{r:?}"))
                .unwrap_or_else(|| node.raw_type.clone());
            if let Some(id) = &node.identifier {
                out.push(SelectorCandidate {
                    spec: format!("id: {id}"),
                    role: role_name.clone(),
                    bounds: bounds_tuple(node),
                });
            }
            if let Some(text) = node.text.as_deref().or(node.value.as_deref()) {
                out.push(SelectorCandidate {
                    spec: format!("text: \"{text}\""),
                    role: role_name.clone(),
                    bounds: bounds_tuple(node),
                });
            }
            if let Some(label) = &node.label {
                out.push(SelectorCandidate {
                    spec: format!("label: \"{label}\""),
                    role: role_name,
                    bounds: bounds_tuple(node),
                });
            }
        }
    });
    out
}

fn bounds_tuple(n: &A11yNode) -> (f64, f64, f64, f64) {
    (n.bounds.x, n.bounds.y, n.bounds.w, n.bounds.h)
}

/// Parse `partial` as `field: pattern` (e.g. `id: qa-*`), or fall back
/// to searching every readable string on the node.
///
/// The fallback used to mean the `text` field alone. On a real iOS tree
/// that is nearly always empty: a capture of Settings has 33 nodes
/// carrying a label, two carrying a value, and zero carrying text or
/// title. So the bare form the help prints — `smix authoring suggest
/// 'Sign In'` — structurally could not match anything, which is what
/// "verify the example against a live runner" turned out to mean.
fn parse_partial(partial: &str) -> (&'static str, &str) {
    if let Some((k, v)) = partial.split_once(':') {
        let k = k.trim();
        let v = v.trim();
        match k {
            "id" => ("id", v),
            "text" => ("text", v),
            "label" => ("label", v),
            _ => ("any", partial),
        }
    } else {
        ("any", partial)
    }
}

fn matches_partial(node: &A11yNode, field: &str, pattern: &str) -> bool {
    // `any` is the bare form: every readable string, label included.
    // Restricting it to `text` made the documented example impossible
    // to satisfy on iOS, where the readable string is the label.
    let candidates: [Option<&str>; 5] = match field {
        "id" => [node.identifier.as_deref(), None, None, None, None],
        "text" => [
            node.text.as_deref(),
            node.value.as_deref(),
            node.title.as_deref(),
            None,
            None,
        ],
        "label" => [node.label.as_deref(), None, None, None, None],
        _ => [
            node.label.as_deref(),
            node.text.as_deref(),
            node.value.as_deref(),
            node.title.as_deref(),
            node.identifier.as_deref(),
        ],
    };
    candidates
        .iter()
        .flatten()
        .any(|hay| matches_one(hay, pattern))
}

fn matches_one(hay: &str, pattern: &str) -> bool {
    // Support `*` wildcard + case-insensitive substring.
    if pattern == "*" {
        return true;
    }
    if let Some(stripped) = pattern.strip_suffix('*') {
        return hay.to_lowercase().starts_with(&stripped.to_lowercase());
    }
    if let Some(stripped) = pattern.strip_prefix('*') {
        return hay.to_lowercase().ends_with(&stripped.to_lowercase());
    }
    hay.to_lowercase().contains(&pattern.to_lowercase())
}

fn walk_tree<F: FnMut(&A11yNode)>(node: &A11yNode, visit: &mut F) {
    visit(node);
    for c in &node.children {
        walk_tree(c, visit);
    }
}

/// Semantic a11y-tree diff.
///
/// Compares `current` against `baseline` and returns a list of
/// structural differences. Missing / new / mismatched nodes are
/// keyed by their (role, identifier) tuple; text/value drift is
/// reported separately.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeDiff {
    /// Nodes present in baseline but missing in current.
    pub missing: Vec<String>,
    /// Nodes present in current but not in baseline.
    pub extra: Vec<String>,
    /// Nodes with same key but drifted text/value/label content.
    pub drifted: Vec<TreeDrift>,
}

/// Drift entry for a node present in both trees.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeDrift {
    pub key: String,
    pub baseline: String,
    pub current: String,
}

impl TreeDiff {
    /// True when trees are structurally identical + no content drift.
    pub fn is_clean(&self) -> bool {
        self.missing.is_empty() && self.extra.is_empty() && self.drifted.is_empty()
    }
}

/// Compute semantic diff of two a11y trees.
pub fn diff_a11y_trees(baseline: &A11yNode, current: &A11yNode) -> TreeDiff {
    let mut base_keys = std::collections::BTreeMap::new();
    let mut curr_keys = std::collections::BTreeMap::new();
    collect_keyed(baseline, &mut base_keys);
    collect_keyed(current, &mut curr_keys);
    let mut missing = Vec::new();
    let mut extra = Vec::new();
    let mut drifted = Vec::new();
    for (k, base_content) in &base_keys {
        match curr_keys.get(k) {
            None => missing.push(k.clone()),
            Some(curr_content) if curr_content != base_content => {
                drifted.push(TreeDrift {
                    key: k.clone(),
                    baseline: base_content.clone(),
                    current: curr_content.clone(),
                });
            }
            _ => {}
        }
    }
    for k in curr_keys.keys() {
        if !base_keys.contains_key(k) {
            extra.push(k.clone());
        }
    }
    TreeDiff {
        missing,
        extra,
        drifted,
    }
}

fn collect_keyed(node: &A11yNode, out: &mut std::collections::BTreeMap<String, String>) {
    // Key: `<role>#<identifier>` when identifier present; else role + label.
    let role = node
        .role
        .as_ref()
        .map(|r| format!("{r:?}"))
        .unwrap_or_else(|| node.raw_type.clone());
    let key = match (&node.identifier, &node.label) {
        (Some(id), _) => format!("{role}#{id}"),
        (None, Some(lbl)) => format!("{role}@{lbl}"),
        _ => role.clone(),
    };
    // Content: text | value | title (first non-empty).
    let content = node
        .text
        .as_deref()
        .or(node.value.as_deref())
        .or(node.title.as_deref())
        .unwrap_or("")
        .to_string();
    // On collision (multiple identical nodes), disambiguate by
    // bounds signature to avoid clobbering.
    let dedup_key = if out.contains_key(&key) {
        format!(
            "{key}@bounds({},{},{}x{})",
            node.bounds.x as i32, node.bounds.y as i32, node.bounds.w as i32, node.bounds.h as i32
        )
    } else {
        key
    };
    out.insert(dedup_key, content);
    for c in &node.children {
        collect_keyed(c, out);
    }
}

// -------------------- CLI dispatch surface ------------------------------

/// Dispatch `smix authoring <action>`.
pub async fn cmd_suggest(port: u16, partial: String) -> Result<ExitCode, CliError> {
    let tree_json = act::fetch_tree_json(port)
        .await
        .map_err(|e| CliError::Other(e.to_string()))?;
    let tree: A11yNode = serde_json::from_value(tree_json)
        .map_err(|e| CliError::Other(format!("parse tree: {e}")))?;
    let candidates = suggest_selectors(&tree, &partial);
    if candidates.is_empty() {
        eprintln!("no candidates found for `{partial}`");
        return Ok(ExitCode::from(1));
    }
    for c in candidates.iter().take(50) {
        println!(
            "  - {}  (role={}, bounds=({:.0},{:.0},{:.0}x{:.0}))",
            c.spec, c.role, c.bounds.0, c.bounds.1, c.bounds.2, c.bounds.3
        );
    }
    Ok(ExitCode::SUCCESS)
}

pub async fn cmd_diff_tree(port: u16, baseline: PathBuf) -> Result<ExitCode, CliError> {
    let base_bytes =
        std::fs::read(&baseline).map_err(|e| CliError::Other(format!("read baseline: {e}")))?;
    let base: A11yNode = serde_json::from_slice(&base_bytes)
        .map_err(|e| CliError::Other(format!("parse baseline: {e}")))?;
    let curr_json = act::fetch_tree_json(port)
        .await
        .map_err(|e| CliError::Other(e.to_string()))?;
    let curr: A11yNode = serde_json::from_value(curr_json)
        .map_err(|e| CliError::Other(format!("parse current tree: {e}")))?;
    let diff = diff_a11y_trees(&base, &curr);
    if diff.is_clean() {
        println!("tree: clean (no missing / extra / drifted)");
        Ok(ExitCode::SUCCESS)
    } else {
        for m in &diff.missing {
            println!("  MISSING: {m}");
        }
        for e in &diff.extra {
            println!("  EXTRA: {e}");
        }
        for d in &diff.drifted {
            println!("  DRIFT: {} \"{}\" → \"{}\"", d.key, d.baseline, d.current);
        }
        Ok(ExitCode::from(2))
    }
}

/// Session recording: capture periodic tree
/// snapshots + emit a yaml scaffold that captures a11y drift over
/// time. Minimal impl: sample at `interval_ms`, write per-sample
/// tree JSONs, emit a yaml with `- assertVisible` steps per stable
/// visible element. Full XCUITest tap-event recording would need
/// runner-side event capture; this session captures enough for
/// authoring iteration.
pub async fn cmd_record_session(
    port: u16,
    duration_secs: u64,
    interval_ms: u64,
    output: PathBuf,
) -> Result<ExitCode, CliError> {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(duration_secs);
    let mut samples: Vec<A11yNode> = Vec::new();
    while std::time::Instant::now() < end {
        let tree_json = act::fetch_tree_json(port)
            .await
            .map_err(|e| CliError::Other(e.to_string()))?;
        let tree: A11yNode = serde_json::from_value(tree_json)
            .map_err(|e| CliError::Other(format!("parse: {e}")))?;
        samples.push(tree);
        tokio::time::sleep(std::time::Duration::from_millis(interval_ms)).await;
    }
    // Generate yaml scaffold from stable IDs across samples.
    let mut stable_ids: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    let n_samples = samples.len();
    for s in &samples {
        let mut seen = std::collections::BTreeSet::new();
        walk_tree(s, &mut |node| {
            if let Some(id) = &node.identifier
                && seen.insert(id.clone())
            {
                *stable_ids.entry(id.clone()).or_insert(0) += 1;
            }
        });
    }
    let mut yaml = String::new();
    yaml.push_str("# auto-generated session recording\n");
    yaml.push_str("appId: com.example    # update to your app\n");
    yaml.push_str("---\n");
    for (id, count) in &stable_ids {
        if *count >= n_samples * 3 / 4 {
            yaml.push_str(&format!("- assertVisible:\n    id: {id}\n"));
        }
    }
    std::fs::write(&output, yaml)
        .map_err(|e| CliError::Other(format!("write {}: {e}", output.display())))?;
    eprintln!(
        "recorded {n_samples} samples over {duration_secs}s; wrote yaml scaffold to {}",
        output.display()
    );
    Ok(ExitCode::SUCCESS)
}

pub async fn cmd_capture_tree(port: u16, out: PathBuf) -> Result<ExitCode, CliError> {
    let tree_json = act::fetch_tree_json(port)
        .await
        .map_err(|e| CliError::Other(e.to_string()))?;
    let bytes = serde_json::to_vec_pretty(&tree_json)
        .map_err(|e| CliError::Other(format!("serialize: {e}")))?;
    std::fs::write(&out, &bytes)
        .map_err(|e| CliError::Other(format!("write {}: {e}", out.display())))?;
    eprintln!("captured a11y tree: {}", out.display());
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use smix_screen::{A11yNode, Rect, Role};

    fn node(id: Option<&str>, text: Option<&str>) -> A11yNode {
        A11yNode {
            raw_type: "any".into(),
            element_type_raw: 1,
            role: Some(Role::Button),
            identifier: id.map(String::from),
            label: None,
            title: None,
            placeholder_value: None,
            value: None,
            text: text.map(String::from),
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 40.0,
            },
            enabled: true,
            selected: false,
            has_focus: false,
            visible: true,
            children: vec![],
        }
    }

    #[test]
    fn suggest_id_wildcard() {
        let root = A11yNode {
            children: vec![
                node(Some("qa-chip-a"), Some("Chip A")),
                node(Some("qa-chip-b"), Some("Chip B")),
                node(Some("other"), Some("Other")),
            ],
            ..node(None, None)
        };
        let out = suggest_selectors(&root, "id: qa-*");
        assert!(out.iter().any(|c| c.spec == "id: qa-chip-a"));
        assert!(out.iter().any(|c| c.spec == "id: qa-chip-b"));
        assert!(!out.iter().any(|c| c.spec == "id: other"));
    }

    #[test]
    fn suggest_text_case_insensitive() {
        let root = A11yNode {
            children: vec![node(None, Some("Sign In")), node(None, Some("Log Out"))],
            ..node(None, None)
        };
        let out = suggest_selectors(&root, "sign");
        assert!(out.iter().any(|c| c.spec == "text: \"Sign In\""));
    }

    #[test]
    fn diff_clean_when_identical() {
        let root = A11yNode {
            children: vec![node(Some("a"), Some("hi"))],
            ..node(None, None)
        };
        let diff = diff_a11y_trees(&root, &root);
        assert!(diff.is_clean());
    }

    #[test]
    fn diff_reports_missing() {
        let base = A11yNode {
            children: vec![node(Some("a"), Some("hi")), node(Some("b"), Some("bye"))],
            ..node(None, None)
        };
        let curr = A11yNode {
            children: vec![node(Some("a"), Some("hi"))],
            ..node(None, None)
        };
        let diff = diff_a11y_trees(&base, &curr);
        assert!(!diff.is_clean());
        assert!(diff.missing.iter().any(|k| k.contains("#b")));
    }

    #[test]
    fn diff_reports_drift() {
        let base = A11yNode {
            children: vec![node(Some("a"), Some("hi"))],
            ..node(None, None)
        };
        let curr = A11yNode {
            children: vec![node(Some("a"), Some("hello"))],
            ..node(None, None)
        };
        let diff = diff_a11y_trees(&base, &curr);
        assert!(!diff.is_clean());
        assert_eq!(diff.drifted.len(), 1);
        assert_eq!(diff.drifted[0].baseline, "hi");
        assert_eq!(diff.drifted[0].current, "hello");
    }
}

#[cfg(test)]
mod authoring_live_tree_tests {
    use super::*;

    /// A tree a real device produced, captured once and committed.
    ///
    /// The ledger recorded `authoring suggest` as needing a live runner
    /// to verify. Running it produced an answer nobody had: the bare
    /// spec form the help prints matched nothing, because it searched
    /// only `text` while every readable string on an iOS node lives on
    /// `label`. This capture is what makes that checkable from now on
    /// without a simulator.
    const LIVE_TREE: &str = include_str!("../tests/fixtures/live-tree-preferences-2026-07-22.json");

    fn live_tree() -> A11yNode {
        serde_json::from_str(LIVE_TREE).expect("captured tree deserializes into A11yNode")
    }

    #[test]
    fn authoring_live_tree_deserializes_into_a11y_node() {
        assert!(
            !live_tree().children.is_empty(),
            "a captured Settings tree with no children means the capture is wrong, not the parser"
        );
    }

    /// The wildcard form from the help. Preferences carries no `qa-`
    /// prefix, so the assertion is that the form survives a real tree —
    /// not that it hits.
    #[test]
    fn authoring_live_tree_suggest_id_wildcard_runs() {
        let _ = suggest_selectors(&live_tree(), "id: qa-*").len();
    }

    /// The bare form from the help, against a label the capture
    /// contains. This is the assertion that was failing on a device
    /// before the bare form learned to look at labels; without it the
    /// test above passes vacuously.
    #[test]
    fn authoring_live_tree_suggest_bare_string_finds_label() {
        let hits = suggest_selectors(&live_tree(), "General");
        assert!(
            !hits.is_empty(),
            "the bare spec form found nothing for a label the capture contains — \
             on iOS the readable string is the label, and 0 of 33 nodes here carry `text`"
        );
    }
}

// ---- record -> generate glue (v2.10-C4) ------------------------------
//
// The single host-side path from recorded IRAction to a flow file. Every
// capture leg (iOS RecordingApp, Android /record, web mapDomEvents) produces
// the same IRAction JSON; this feeds it to the one platform-neutral generator
// stone (`smix-recorder`), so a recording from any platform becomes a flow.

/// Output format for `smix authoring generate`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum GenFormat {
    Maestro,
    Rust,
}

/// Deserialize recorded `Vec<IRAction>` JSON and generate a flow. Pure — the
/// single source of generation is the `smix-recorder` stone.
pub fn generate_actions_json(
    input: &[u8],
    format: GenFormat,
    app_id: &str,
    test_fn_name: &str,
) -> Result<String, CliError> {
    let actions: Vec<smix_recorder::IRAction> = serde_json::from_slice(input)
        .map_err(|e| CliError::Other(format!("parse IRAction JSON: {e}")))?;
    match format {
        GenFormat::Maestro => smix_recorder::generate_maestro_yaml(&actions, app_id)
            .map_err(|e| CliError::Other(format!("generate maestro: {e}"))),
        GenFormat::Rust => smix_recorder::generate_rust(&actions, test_fn_name, app_id)
            .map_err(|e| CliError::Other(format!("generate rust: {e}"))),
    }
}

/// `smix authoring generate` — read recorded IRAction JSON and write a flow.
pub async fn cmd_generate(
    input: PathBuf,
    format: GenFormat,
    output: PathBuf,
    app_id: String,
    test_fn_name: String,
) -> Result<ExitCode, CliError> {
    let bytes = std::fs::read(&input)
        .map_err(|e| CliError::Other(format!("read {}: {e}", input.display())))?;
    let out = generate_actions_json(&bytes, format, &app_id, &test_fn_name)?;
    std::fs::write(&output, &out)
        .map_err(|e| CliError::Other(format!("write {}: {e}", output.display())))?;
    println!("wrote {}", output.display());
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod authoring_generate_tests {
    use super::{GenFormat, generate_actions_json};

    const ACTIONS: &str = concat!(
        r#"[{"kind":"tap","selector":{"id":"go"},"timestampMs":1},"#,
        r#"{"kind":"fill","selector":{"id":"q"},"text":"smix","timestampMs":2},"#,
        r#"{"kind":"clear","selector":{"id":"q"},"timestampMs":3}]"#
    );

    #[test]
    fn maestro_carries_the_actions() {
        let y = generate_actions_json(ACTIONS.as_bytes(), GenFormat::Maestro, "com.x", "recorded")
            .expect("maestro");
        assert!(y.contains("tapOn"), "{y}");
        assert!(y.contains("inputText"), "{y}");
    }

    #[test]
    fn rust_is_a_test_fn() {
        let r = generate_actions_json(ACTIONS.as_bytes(), GenFormat::Rust, "com.x", "recorded")
            .expect("rust");
        assert!(r.contains("async fn recorded"), "{r}");
    }

    #[test]
    fn empty_actions_is_an_error_not_a_swallow() {
        assert!(generate_actions_json(b"[]", GenFormat::Maestro, "com.x", "recorded").is_err());
    }

    #[test]
    fn bad_json_errors() {
        assert!(
            generate_actions_json(b"not json", GenFormat::Maestro, "com.x", "recorded").is_err()
        );
    }
}

/// `smix authoring propose` — device-free wire over `propose_and_amend`: read
/// a failed flow + its on-disk bundle, ask the local `claude` to propose
/// edits, write the amended flow yaml. Producing the bundle (running the flow
/// on a device) is the caller's step, not this one.
pub async fn cmd_propose(
    flow: PathBuf,
    bundle: PathBuf,
    output: PathBuf,
) -> Result<ExitCode, CliError> {
    let cfg = smix_authoring_propose::AiTierConfig::default();
    let yaml = smix_authoring_propose::propose_and_amend(&flow, &bundle, &cfg)
        .await
        .map_err(|e| CliError::Other(format!("propose: {e:?}")))?;
    std::fs::write(&output, &yaml)
        .map_err(|e| CliError::Other(format!("write {}: {e}", output.display())))?;
    println!("wrote {}", output.display());
    Ok(ExitCode::SUCCESS)
}

/// `smix authoring tap-record` — record a live session on a runner and generate
/// a flow. Starts recording, waits `duration_secs` while the operator drives
/// the app, stops and drains the IRAction the capture leg emitted, and feeds it
/// to the same generate glue. Android today (the Android runner emits IRAction);
/// iOS live-user-capture -> generate is a v2.11+ follow-on (its /record emits
/// RecordedEvent, which carries no action intent).
pub async fn cmd_tap_record(
    port: u16,
    duration_secs: u64,
    format: GenFormat,
    output: PathBuf,
    app_id: String,
    test_fn_name: String,
) -> Result<ExitCode, CliError> {
    let client = smix_runner_client::HttpRunnerClient::new(port);
    client
        .start_record()
        .await
        .map_err(|e| CliError::Other(format!("record start: {e}")))?;
    println!("recording for {duration_secs}s on port {port} — drive the app now…");
    tokio::time::sleep(std::time::Duration::from_secs(duration_secs)).await;
    let events = client
        .stop_record_actions()
        .await
        .map_err(|e| CliError::Other(format!("record stop: {e}")))?;
    let json = serde_json::to_vec(&events)
        .map_err(|e| CliError::Other(format!("serialize events: {e}")))?;
    let out = generate_actions_json(&json, format, &app_id, &test_fn_name)?;
    std::fs::write(&output, &out)
        .map_err(|e| CliError::Other(format!("write {}: {e}", output.display())))?;
    println!(
        "recorded {} action(s); wrote {}",
        events.len(),
        output.display()
    );
    Ok(ExitCode::SUCCESS)
}
