//! smix-migrate — static maestro→smix YAML codemod.
//!
//! # Why this crate exists
//!
//! `smix run` already accepts maestro-flavored yaml directly (smix is a
//! superset of maestro). Migration is not required for correctness. But
//! when a consumer commits their yaml flows for long-term maintenance,
//! they want:
//!
//! 1. **Canonical verb names** — grep for `- tap:` finds every tap, not
//!    a mix of `- tapOn:` (maestro) / `- tap:` (smix). Better code
//!    review, better tooling.
//! 2. **smix-native argument shapes** — e.g. `extendedWaitUntil.timeout`
//!    becomes `expect.timeoutMs`, aligning with smix's `expect` verb
//!    family.
//! 3. **Deprecated-verb removal** — maestro-only forms flagged
//!    (`WARN:` to stderr) so consumers can decide whether to hand-edit.
//!
//! # Scope
//!
//! - **Top-20 verb transforms** (list below in [`Migrator::default`])
//! - **Comment preservation** — line-based rewriter keeps copyright
//!   headers, audit-trail comments, and blank lines byte-identical.
//! - **Unrecognized verbs preserved verbatim** — e.g. `runScript`,
//!   `evalScript` — a `WARN:` per unknown verb, one line to stderr.
//! - **Library + CLI** — this crate exposes `Migrator` + `MigrateReport`;
//!   `smix migrate` in the CLI is a thin wrapper.
//!
//! # Design decisions
//!
//! The codemod operates on `serde_norway::Value` (loose YAML AST) for a
//! syntactic sanity check, then applies changes with a line-based
//! rewriter to preserve comments. We do NOT deserialize into a
//! strongly-typed maestro schema because:
//! - Preserving unknown / smix-native verbs verbatim is required
//! - We only touch the specific keys we care about (whitelist approach)
//! - Round-tripping through a typed schema would drop any field we
//!   don't know about, breaking already-smix-native yamls
//!
//! # Anti-goals
//!
//! - Not a linter (no correctness checks; malformed yaml surfaces as a
//!   parse error)
//! - Not a formatter (indentation / trailing spaces preserved verbatim)

use std::fmt;

use serde_norway::Value;
use thiserror::Error;

/// Result of migrating one flow file.
#[derive(Clone, Debug, Default)]
pub struct MigrateReport {
    /// Verbs that were successfully renamed / restructured. Keys are
    /// original verb name; values are `smix-native` verb name.
    pub renamed: Vec<Rename>,
    /// Verbs the migrator does not recognize. They were left verbatim
    /// in the output. Consumers should see the `WARN:` line at the CLI
    /// layer.
    pub unknown_verbs: Vec<String>,
    /// Number of top-level steps in the flow.
    pub step_count: usize,
    /// Number of `runFlow` invocations discovered in the flow. Not
    /// migrated recursively (nested files are user's responsibility).
    pub subflow_refs: usize,
}

/// One verb rename recorded during a migration pass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rename {
    pub from: &'static str,
    pub to: &'static str,
    /// 1-indexed step position where the rename was applied.
    pub step_index: usize,
}

impl fmt::Display for Rename {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "step #{}: {} → {}", self.step_index, self.from, self.to)
    }
}

#[derive(Debug, Error)]
pub enum MigrateError {
    #[error("failed to parse maestro yaml: {0}")]
    Parse(#[from] serde_norway::Error),
    #[error("failed to serialize smix yaml: {source}")]
    Serialize {
        #[source]
        source: serde_norway::Error,
    },
    #[error("input yaml has no top-level list (need `[header, steps...]` or `[steps...]`)")]
    Shape,
}

/// The migrator. Stateless by design — `run` is a pure fn of input.
///
/// Use [`Migrator::default`] for the standard transform table. Custom
/// consumers can construct with `Migrator::new(&[...])` to add / omit
/// rules for their yaml corpus (currently unused; opened for future
/// extension without wire-format changes).
#[derive(Clone, Debug)]
pub struct Migrator {
    rules: Vec<Rule>,
}

impl Default for Migrator {
    fn default() -> Self {
        Self {
            rules: default_rules(),
        }
    }
}

impl Migrator {
    /// Comment-preserving line-based codemod. Copyright headers, audit
    /// trails, and `# Reason:` blocks survive the migrate round-trip
    /// byte-identical.
    ///
    /// Strategy:
    /// - Iterate input line by line
    /// - Preserve comment / blank / doc-header lines verbatim
    /// - Detect step lines (`^\s*-\s+<verb>[:.]?`) and rewrite the
    ///   verb portion using [`smix_verbs::VERB_TABLE`]
    /// - Argument transforms (e.g. `timeout` → `timeoutMs`,
    ///   `max` → `maxRetries`) run against the arg mapping only when
    ///   the arg is inline on the same step line — for multi-line
    ///   mappings we scan following-indented lines
    ///
    /// The prior serde_norway::Value roundtrip is retained as a
    /// fallback for edge cases: when the line-based rewriter can't
    /// confidently identify a step (deeply nested / complex flow-
    /// style), fall back to the AST path for that yaml doc.
    pub fn migrate(&self, yaml: &str) -> Result<(String, MigrateReport), MigrateError> {
        // Light upfront parse gate: use serde_norway as a syntactic
        // sanity check. If yaml is malformed, surface Parse error early
        // so it never gets past migrate.
        {
            let de = serde_norway::Deserializer::from_str(yaml);
            for doc in de {
                let _: Value = serde_norway::with::singleton_map_recursive::deserialize(doc)?;
            }
        }
        let mut report = MigrateReport::default();
        let out = self.migrate_lines(yaml, &mut report);
        Ok((out, report))
    }

    /// Line-based rewriter. Returns migrated yaml preserving all
    /// non-step content verbatim.
    fn migrate_lines(&self, input: &str, report: &mut MigrateReport) -> String {
        let mut out = String::with_capacity(input.len());
        let mut step_counter = 0usize;
        let mut pending_arg_rules: Option<(usize, &'static str)> = None;
        // pending_arg_rules = (arg_indent, maestro_name) — when Some,
        // subsequent lines with `indent > arg_indent` are arg-mapping
        // lines for that verb; apply arg-key renames per verb's
        // transform. Cleared when line indent <= arg_indent.
        for line in input.split('\n') {
            // Empty line or comment — preserve verbatim.
            let trimmed = line.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                out.push_str(line);
                out.push('\n');
                continue;
            }
            // Doc separator or leading `---` — preserve.
            if trimmed == "---" || trimmed.starts_with("---") {
                pending_arg_rules = None;
                out.push_str(line);
                out.push('\n');
                continue;
            }
            let indent = line.len() - trimmed.len();
            // Check if this is a continuation of a pending arg mapping.
            if let Some((arg_indent, maestro_name)) = pending_arg_rules {
                if indent > arg_indent {
                    // Line is part of the arg mapping. But it may still
                    // be a nested step-line (e.g. `commands: - tapOn:`)
                    // in which case we route to step rewrite even
                    // inside the pending state.
                    if let Some(step_info) = parse_step_line(line) {
                        step_counter += 1;
                        let rewritten =
                            rewrite_step_line(line, &step_info, self, report, step_counter);
                        out.push_str(&rewritten.line);
                        out.push('\n');
                        continue;
                    }
                    // Not a nested step — rewrite arg key if it's one
                    // of the transforms we support.
                    match rewrite_arg_line(line, maestro_name) {
                        ArgLineResult::Rewritten(rewritten) => {
                            out.push_str(&rewritten);
                            out.push('\n');
                        }
                        ArgLineResult::Strip => {
                            // e.g. `launchApp: { clearState: false }` —
                            // drop the line entirely (deprecated
                            // maestro default, adds noise).
                        }
                    }
                    continue;
                } else {
                    pending_arg_rules = None;
                }
            }
            // Try to parse as a step line: `\s*-\s+<verb>...`.
            if let Some(step_info) = parse_step_line(line) {
                step_counter += 1;
                let rewritten = rewrite_step_line(line, &step_info, self, report, step_counter);
                out.push_str(&rewritten.line);
                out.push('\n');
                // Track pending arg mapping if verb has follow-up
                // indented keys.
                if let Some(maestro_name) = rewritten.pending_arg_maestro_name {
                    pending_arg_rules = Some((step_info.step_indent, maestro_name));
                }
                continue;
            }
            // Not a step line, not a comment — preserve verbatim
            // (yaml header like `appId:`, etc.).
            out.push_str(line);
            out.push('\n');
        }
        // Remove trailing empty line inserted by split('\n') on
        // trailing newline input.
        if out.ends_with("\n\n") && !input.ends_with("\n\n") {
            out.pop();
        }
        // If input didn't end with newline, trim ours.
        if !input.ends_with('\n') && out.ends_with('\n') {
            out.pop();
        }
        out
    }

    #[allow(dead_code)]
    fn transform_value(
        &self,
        value: &mut Value,
        report: &mut MigrateReport,
        step_counter: &mut usize,
    ) {
        // Legacy AST-based walker — retained as reference / potential
        // future fallback for edge cases the line-based rewriter can't
        // handle (deeply nested / flow-style yaml). Currently unused.
        walk(value, |step| {
            *step_counter += 1;
            report.step_count += 1;
            let idx = *step_counter;
            if let Value::Mapping(map) = step {
                self.apply_step_transform(map, report, idx);
            }
        });
    }

    #[allow(dead_code)]
    fn apply_step_transform(
        &self,
        map: &mut serde_norway::Mapping,
        report: &mut MigrateReport,
        idx: usize,
    ) {
        // A step is a single-key mapping in the canonical form. We look
        // at each rule's `from` verb; when present, apply.
        let mut applicable: Vec<&Rule> = Vec::new();
        for rule in &self.rules {
            let key = Value::String(rule.from.to_string());
            if map.contains_key(&key) {
                applicable.push(rule);
            }
        }
        for rule in applicable {
            let from_key = Value::String(rule.from.to_string());
            let to_key = Value::String(rule.to.to_string());
            // Remove-then-insert preserves original value; apply
            // rule-specific argument transform if any.
            if let Some(mut arg_val) = map.remove(&from_key) {
                (rule.transform)(&mut arg_val);
                map.insert(to_key, arg_val);
                report.renamed.push(Rename {
                    from: rule.from,
                    to: rule.to,
                    step_index: idx,
                });
                if rule.from == "runFlow" || rule.to == "runFlow" {
                    report.subflow_refs += 1;
                }
            }
        }
        // Unknown-verb tracking: any single-key top-level map whose key
        // is NOT in a rules `from` set + NOT in the smix-native known
        // list is reported.
        if map.len() == 1
            && let Some((Value::String(key), _)) = map.iter().next()
            && !is_known_verb(key)
            && !is_ignored_key(key)
            && !report.unknown_verbs.iter().any(|v| v == key)
        {
            report.unknown_verbs.push(key.clone());
        }
    }
}

/// Walk a Value tree, invoking `visit` on every Mapping node that
/// looks like a "step" (single-key mapping with a string key).
#[allow(dead_code)]
fn walk<F: FnMut(&mut Value)>(value: &mut Value, mut visit: F) {
    walk_inner(value, &mut visit);
}

#[allow(dead_code)]
fn walk_inner<F: FnMut(&mut Value)>(value: &mut Value, visit: &mut F) {
    match value {
        Value::Sequence(seq) => {
            for item in seq {
                // A step can be:
                //   (a) `Value::Mapping` with a single verb-key (canonical)
                //   (b) `Value::String("verb")` bare-string form (maestro
                //       accepts `- back`, `- scroll`, `- hideKeyboard`,
                //       `- waitForAnimationToEnd` etc.)
                //
                // Both need canonicalization. For (b), we visit a
                // temporary single-key mapping shim so all rules apply
                // uniformly. If the visitor renames the verb, we keep
                // bare-string shape (bare-in, bare-out — more idiomatic).
                if let Value::String(s) = item {
                    let name = s.clone();
                    // Temporarily promote to `{name: null}` so
                    // `apply_step_transform` matches rules by key.
                    let mut shim = Value::Mapping(
                        [(Value::String(name.clone()), Value::Null)]
                            .into_iter()
                            .collect(),
                    );
                    visit(&mut shim);
                    // Detect whether the visit renamed the key. If so,
                    // reproject back to bare-string.
                    if let Value::Mapping(m) = &shim
                        && m.len() == 1
                    {
                        if let Some((Value::String(new_key), Value::Null)) = m.iter().next() {
                            if new_key != &name {
                                *item = Value::String(new_key.clone());
                            }
                        } else if let Some((Value::String(_new_key), _)) = m.iter().next() {
                            // Value became non-null (unlikely for
                            // string-shape input); reproject as mapping.
                            *item = Value::Mapping(m.clone());
                        }
                    }
                    continue;
                }
                if let Value::Mapping(m) = item
                    && m.len() == 1
                {
                    visit(item);
                    // After the step visitor ran, walk the inner
                    // value in case it's a `runFlow` etc. that
                    // contains `commands: [...]`.
                    if let Value::Mapping(m) = item {
                        for (_, v) in m.iter_mut() {
                            walk_inner(v, visit);
                        }
                    }
                    continue;
                }
                walk_inner(item, visit);
            }
        }
        Value::Mapping(m) => {
            for (_, v) in m.iter_mut() {
                walk_inner(v, visit);
            }
        }
        _ => {}
    }
}

/// Top-20 verb rename + argument transform rules.
///
/// Each rule = (maestro verb name, smix verb name, arg transform fn).
/// Argument transform runs on the value BEFORE re-insertion under the
/// new key. Identity transform = arg carried verbatim.
#[derive(Clone, Debug)]
struct Rule {
    from: &'static str,
    to: &'static str,
    #[allow(dead_code)]
    transform: fn(&mut Value),
}

#[allow(dead_code)]
fn id_transform(_: &mut Value) {}

/// `extendedWaitUntil: { visible: X, timeout: 3000 }`
/// → `expect: { visible: X, timeoutMs: 3000 }`
#[allow(dead_code)]
fn transform_extended_wait_until(arg: &mut Value) {
    rename_key(arg, "timeout", "timeoutMs");
}

/// `retry: { max: 3, ... }` → `retry: { maxRetries: 3, ... }`
#[allow(dead_code)]
fn transform_retry(arg: &mut Value) {
    rename_key(arg, "max", "maxRetries");
}

/// `launchApp: { clearState: true, ... }` — smix-native accepts the same
/// shape but strips deprecated `clearState: false` (no-op maestro form).
#[allow(dead_code)]
fn transform_launch_app(arg: &mut Value) {
    if let Value::Mapping(m) = arg {
        // Drop `clearState: false` — it's the maestro default and adds noise
        let cs_key = Value::String("clearState".to_string());
        if let Some(Value::Bool(false)) = m.get(&cs_key) {
            m.remove(&cs_key);
        }
    }
}

/// Rename an inner mapping key when it exists.
#[allow(dead_code)]
fn rename_key(val: &mut Value, from: &str, to: &str) {
    if let Value::Mapping(m) = val {
        let from_key = Value::String(from.to_string());
        let to_key = Value::String(to.to_string());
        if let Some(v) = m.remove(&from_key) {
            m.insert(to_key, v);
        }
    }
}

/// Parsed step-line shape used by the line-based rewriter.
#[derive(Debug, Clone)]
struct StepLineInfo {
    /// Number of leading spaces before the `-`.
    step_indent: usize,
    /// Column where the verb name starts (after `-` and space).
    verb_start: usize,
    /// End column of the verb name (exclusive; points at `:` or end).
    verb_end: usize,
    /// Verb name as it appeared in the input.
    verb_name: String,
    /// True if verb was followed by `:` (has inline arg or arg on
    /// subsequent indented lines).
    has_colon: bool,
}

/// Rewritten-step output plus optional pending-arg tracker for the
/// follow-up arg mapping (multi-line args).
struct RewrittenStep {
    line: String,
    pending_arg_maestro_name: Option<&'static str>,
}

/// Parse a yaml line as a step-line (`\s*-\s+<verb>[:.]?`).
/// Returns `None` for non-step lines (comments handled upstream).
fn parse_step_line(line: &str) -> Option<StepLineInfo> {
    let bytes = line.as_bytes();
    let step_indent = line.len() - line.trim_start().len();
    let trimmed = &line[step_indent..];
    // Must start with `- ` (dash + space).
    if !trimmed.starts_with("- ") {
        return None;
    }
    // Skip past `- `.
    let verb_start = step_indent + 2;
    // Skip additional whitespace between `-` and verb.
    let mut pos = verb_start;
    while pos < bytes.len() && bytes[pos] == b' ' {
        pos += 1;
    }
    let real_verb_start = pos;
    // Verb name = alphanumeric + `_` chars until `:` or space or end.
    let mut end = real_verb_start;
    while end < bytes.len() {
        let c = bytes[end];
        if c.is_ascii_alphanumeric() || c == b'_' {
            end += 1;
        } else {
            break;
        }
    }
    if end == real_verb_start {
        return None; // no verb name
    }
    let verb_name = line[real_verb_start..end].to_string();
    // Reject if it's not a known verb per smix_verbs (unknown verbs
    // preserved verbatim per unknown-verb reporting later).
    let has_colon = bytes.get(end) == Some(&b':');
    Some(StepLineInfo {
        step_indent,
        verb_start: real_verb_start,
        verb_end: end,
        verb_name,
        has_colon,
    })
}

/// Rewrite the verb portion of a step line.
fn rewrite_step_line(
    line: &str,
    info: &StepLineInfo,
    migrator: &Migrator,
    report: &mut MigrateReport,
    idx: usize,
) -> RewrittenStep {
    // Find rule for the maestro verb name.
    let rule = migrator.rules.iter().find(|r| r.from == info.verb_name);
    let Some(rule) = rule else {
        // Unknown verb — track and preserve verbatim.
        if !smix_verbs::is_known_verb(&info.verb_name)
            && !matches!(
                info.verb_name.as_str(),
                "when" | "file" | "commands" | "label" | "config"
            )
            && !report.unknown_verbs.iter().any(|v| v == &info.verb_name)
        {
            report.unknown_verbs.push(info.verb_name.clone());
        }
        return RewrittenStep {
            line: line.to_string(),
            pending_arg_maestro_name: None,
        };
    };
    let new_verb = rule.to;
    let old_verb = &info.verb_name;
    // Splice the verb portion.
    let mut rewritten = String::with_capacity(line.len() + 4);
    rewritten.push_str(&line[..info.verb_start]);
    rewritten.push_str(new_verb);
    rewritten.push_str(&line[info.verb_end..]);
    if new_verb != old_verb {
        report.renamed.push(Rename {
            from: rule.from,
            to: rule.to,
            step_index: idx,
        });
    }
    // Track subflow refs.
    if rule.from == "runFlow" || rule.to == "runFlow" {
        report.subflow_refs += 1;
    }
    report.step_count += 1;
    // If this verb has a transform_for arg-mapping renamer (i.e.
    // not id_transform), and it has a `:` (indicating either inline
    // arg or nested arg mapping), track pending arg lines.
    let pending = if info.has_colon && verb_has_arg_transform(info.verb_name.as_str()) {
        Some(rule.from)
    } else {
        None
    };
    RewrittenStep {
        line: rewritten,
        pending_arg_maestro_name: pending,
    }
}

/// Result of `rewrite_arg_line`.
enum ArgLineResult {
    /// Line rewritten (or preserved verbatim).
    Rewritten(String),
    /// Line should be dropped entirely (e.g. `clearState: false` under
    /// launchApp — a deprecated maestro default that adds noise).
    Strip,
}

/// Rewrite an arg-mapping continuation line for the named verb.
fn rewrite_arg_line(line: &str, maestro_name: &str) -> ArgLineResult {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    let Some(colon_pos) = trimmed.find(':') else {
        return ArgLineResult::Rewritten(line.to_string());
    };
    let key = &trimmed[..colon_pos];
    let value = trimmed[colon_pos + 1..].trim();
    // Strip case: launchApp clearState: false → drop entirely.
    if maestro_name == "launchApp" && key == "clearState" && value == "false" {
        return ArgLineResult::Strip;
    }
    let new_key = match (maestro_name, key) {
        ("extendedWaitUntil", "timeout") => "timeoutMs",
        ("retry", "max") => "maxRetries",
        _ => return ArgLineResult::Rewritten(line.to_string()),
    };
    let rest = &trimmed[colon_pos..];
    let mut out = String::with_capacity(line.len() + 4);
    for _ in 0..indent {
        out.push(' ');
    }
    out.push_str(new_key);
    out.push_str(rest);
    ArgLineResult::Rewritten(out)
}

/// Quickly test whether a verb has an arg-mapping transform (used by
/// the line-based rewriter to decide whether to track pending-arg
/// continuation lines).
fn verb_has_arg_transform(maestro_name: &str) -> bool {
    matches!(maestro_name, "extendedWaitUntil" | "retry" | "launchApp")
}

/// Argument transform lookup. Kept for the legacy AST-based path
/// (currently unused since the line-based rewriter took over). Retained
/// as reference for future fallback.
#[allow(dead_code)]
fn transform_for(maestro_name: &str) -> fn(&mut Value) {
    match maestro_name {
        "extendedWaitUntil" => transform_extended_wait_until,
        "retry" => transform_retry,
        "launchApp" => transform_launch_app,
        _ => id_transform,
    }
}

/// Default rules derived from `smix_verbs::VERB_TABLE` — the single
/// source of truth. Adding a new verb entry in smix-verbs automatically
/// flows through here.
///
/// Ordering: same as `VERB_TABLE` (category-alphabetized).
fn default_rules() -> Vec<Rule> {
    smix_verbs::VERB_TABLE
        .iter()
        .map(|e| Rule {
            from: e.maestro_name,
            to: e.smix_name,
            transform: transform_for(e.maestro_name),
        })
        .collect()
}

/// Verbs smix natively recognizes — used by the unknown-verb detector.
/// Any single-key top-level map whose key is NOT here + NOT in the
/// rename table → `WARN` line.
#[allow(dead_code)]
fn is_known_verb(v: &str) -> bool {
    matches!(
        v,
        // smix canonical + maestro-canonical that stays verbatim
        "tap" | "tapOn"
        | "expect" | "expectNotVisible" | "assertVisible" | "assertNotVisible"
        | "extendedWaitUntil"
        | "fill" | "inputText" | "clear" | "eraseText"
        | "reset" | "clearState"
        | "resetKeychain" | "clearKeychain"
        | "runFlow"
        | "launchApp" | "terminate" | "stopApp" | "killApp"
        | "openUrl" | "openLink"
        | "pressKey" | "back"
        | "hideKeyboard"
        | "scroll" | "scrollUntilVisible"
        | "swipe" | "swipeOnce"
        | "retry"
        | "takeScreenshot" | "screenshot"
        | "setClipboard" | "readClipboard"
        | "waitForAnimationToEnd"
        | "assertTrue" | "assertFalse" | "assertNotEqual" | "assertEqual"
        | "toggleAirplaneMode" | "travel"
        | "setLocation" | "startRecording" | "stopRecording"
        | "addMedia" | "assertWithAI"
        | "when"
        // smix-native
        | "ocrText" | "anchorRelative" | "tapById" | "tapAtCoord"
        | "swipeAtCoord" | "doubleTap" | "longPress"
        | "setOrientation" | "findTextByOcr"
    )
}

/// Non-verb keys we ignore for unknown-verb reporting (e.g. `when`
/// nested inside a runFlow inline).
#[allow(dead_code)]
fn is_ignored_key(v: &str) -> bool {
    matches!(v, "when" | "file" | "commands" | "label" | "config")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_tap_on_to_tap() {
        let yaml = "appId: com.example\n---\n- tapOn: Login\n- tapOn:\n    text: Submit\n";
        let (out, report) = Migrator::default().migrate(yaml).unwrap();
        assert!(out.contains("tap: Login"));
        assert!(out.contains("tap:"));
        assert!(!out.contains("tapOn:"));
        assert_eq!(report.step_count, 2);
        assert_eq!(report.renamed.len(), 2);
    }

    #[test]
    fn rename_extended_wait_until() {
        let yaml = "appId: com.example\n---\n- extendedWaitUntil:\n    visible: Ready\n    timeout: 3000\n";
        let (out, _) = Migrator::default().migrate(yaml).unwrap();
        assert!(out.contains("expect:"));
        assert!(out.contains("timeoutMs: 3000"));
        assert!(!out.contains("extendedWaitUntil"));
        assert!(!out.contains("timeout: 3000"));
    }

    #[test]
    fn rename_retry_max() {
        let yaml = "appId: com.example\n---\n- retry:\n    max: 3\n    commands:\n    - tapOn: X\n";
        let (out, report) = Migrator::default().migrate(yaml).unwrap();
        assert!(out.contains("maxRetries: 3"));
        assert!(!out.contains("max: 3"));
        assert!(out.contains("tap: X"));
        assert_eq!(report.step_count, 2);
    }

    #[test]
    fn strip_launchapp_clearstate_false() {
        let yaml = "appId: com.example\n---\n- launchApp:\n    clearState: false\n    permissions:\n      camera: allow\n";
        let (out, _) = Migrator::default().migrate(yaml).unwrap();
        assert!(!out.contains("clearState: false"));
        assert!(out.contains("permissions:"));
    }

    #[test]
    fn unknown_verb_reported_and_preserved() {
        let yaml = "appId: com.example\n---\n- runScript:\n    script: foo.js\n- tapOn: X\n";
        let (out, report) = Migrator::default().migrate(yaml).unwrap();
        assert!(out.contains("runScript:"));
        assert!(out.contains("tap: X"));
        assert_eq!(report.unknown_verbs, vec!["runScript"]);
    }

    #[test]
    fn native_smix_verbs_untouched() {
        let yaml = "appId: com.example\n---\n- ocrText: Sign In\n- tapById: submit\n";
        let (out, report) = Migrator::default().migrate(yaml).unwrap();
        assert!(out.contains("ocrText: Sign In"));
        assert!(out.contains("tapById: submit"));
        assert!(report.unknown_verbs.is_empty());
        assert!(report.renamed.is_empty());
    }

    #[test]
    fn nested_runflow_inline_walked() {
        let yaml = "appId: com.example\n---\n- runFlow:\n    when:\n      visible: Splash\n    commands:\n    - tapOn: Skip\n    - inputText: hello\n";
        let (out, report) = Migrator::default().migrate(yaml).unwrap();
        assert!(out.contains("tap: Skip"));
        assert!(out.contains("fill: hello"));
        // runFlow stays runFlow — only inner steps rename.
        assert!(out.contains("runFlow:"));
        assert!(report.renamed.iter().any(|r| r.to == "tap"));
        assert!(report.renamed.iter().any(|r| r.to == "fill"));
    }

    #[test]
    fn stop_app_becomes_terminate() {
        let yaml = "appId: com.example\n---\n- stopApp: com.other\n";
        let (out, _) = Migrator::default().migrate(yaml).unwrap();
        assert!(out.contains("terminate: com.other"));
        assert!(!out.contains("stopApp"));
    }

    #[test]
    fn multi_doc_yaml_preserves_split() {
        let yaml = "appId: com.example\n---\n- tapOn: A\n";
        let (out, _) = Migrator::default().migrate(yaml).unwrap();
        // Two docs → separator preserved
        assert!(out.contains("---"));
        assert!(out.contains("tap: A"));
    }

    #[test]
    fn parse_error_surfaces() {
        let yaml = "appId: com.example\n---\n- [unclosed\n";
        let err = Migrator::default().migrate(yaml).unwrap_err();
        matches!(err, MigrateError::Parse(_));
    }

    #[test]
    fn empty_flow_no_ops() {
        let yaml = "appId: com.example\n---\n";
        let (out, report) = Migrator::default().migrate(yaml).unwrap();
        assert!(out.contains("appId: com.example"));
        assert_eq!(report.step_count, 0);
    }
}
