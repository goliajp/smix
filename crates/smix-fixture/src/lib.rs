//! smix-fixture — fixture chip registry.
//!
//! # Purpose
//!
//! The `- fixture: <id>` yaml verb (in smix-adapter-maestro) looks up
//! `<id>` in a JSON registry to find:
//!
//! - `testID` — the a11y id of the chip in the QA overlay to tap
//! - `signal` — the log line regex the chip emits when its seed
//!   operation completes
//! - `timeoutMs` — how long to wait for the signal
//!
//! smix reads the registry once per run from a path resolved via
//! `.smix/config.yaml` `fixturesRegistry` field.
//!
//! # Format
//!
//! JSON:
//!
//! ```json
//! {
//!   "version": 1,
//!   "fixtures": {
//!     "prime-search-history": {
//!       "testID": "qa-chip-prime-search-history",
//!       "signal": {
//!         "regex": "\\[fixture\\] prime-search-history: seeded (\\d+) rows",
//!         "level": "log"
//!       },
//!       "timeoutMs": 8000
//!     }
//!   }
//! }
//! ```

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// One fixture chip declaration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixtureDecl {
    /// a11y id of the chip to tap in the QA overlay. Serialized as
    /// `testID` (all-caps ID) to match the consumer-side registry
    /// convention.
    #[serde(rename = "testID")]
    pub test_id: String,
    /// Log signal the chip emits when seeding completes.
    pub signal: SignalMatcher,
    /// Timeout in milliseconds. Runtime awaits `signal` up to this.
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: u64,
}

/// Signal matcher (mirrors `smix_metro_log::SignalMatcher` at the wire
/// level).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignalMatcher {
    pub regex: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
}

/// Registry file shape.
#[derive(Debug, Deserialize)]
struct RegistryFile {
    #[serde(default)]
    #[allow(dead_code)]
    version: u32,
    fixtures: BTreeMap<String, FixtureDecl>,
}

/// Loaded registry.
#[derive(Clone, Debug)]
pub struct FixtureRegistry {
    fixtures: BTreeMap<String, FixtureDecl>,
}

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("read {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("parse {path}: {detail}")]
    Malformed { path: String, detail: String },
    #[error("fixture not found: {id} (registered: {known:?})")]
    NotFound { id: String, known: Vec<String> },
}

impl FixtureRegistry {
    /// Load a registry file. Auto-detects format by extension:
    /// - `.json` → parsed as JSON
    /// - `.ts` / `.tsx` → lightweight TypeScript
    ///   `export const FIXTURES = { ... }` extractor (no swc / oxc
    ///   dep — handles the common documented shape)
    /// - anything else → try JSON first, then TS extractor as fallback
    pub fn load(path: &Path) -> Result<Self, RegistryError> {
        let text = std::fs::read_to_string(path).map_err(|source| RegistryError::Io {
            path: path.display().to_string(),
            source,
        })?;
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase());
        match ext.as_deref() {
            Some("ts") | Some("tsx") => Self::load_from_ts_source(&text, path),
            Some("json") => Self::load_from_json(&text, path),
            _ => {
                // Try JSON first; fall back to TS extractor.
                Self::load_from_json(&text, path)
                    .or_else(|_| Self::load_from_ts_source(&text, path))
            }
        }
    }

    fn load_from_json(text: &str, path: &Path) -> Result<Self, RegistryError> {
        let file: RegistryFile =
            serde_json::from_str(text).map_err(|e| RegistryError::Malformed {
                path: path.display().to_string(),
                detail: e.to_string(),
            })?;
        Ok(Self {
            fixtures: file.fixtures,
        })
    }

    /// Lightweight TS `export const FIXTURES = {...}` extractor.
    /// Matches the documented shape:
    /// ```ts
    /// export const FIXTURES: Record<FixtureId, FixtureDecl> = {
    ///   'prime-search-history': {
    ///     testID: 'qa-chip-prime-search-history',
    ///     signal: { level: 'log', regex: /\[fixture\] .../ },
    ///     timeoutMs: 8000,
    ///   },
    ///   // ...
    /// }
    /// ```
    ///
    /// Strategy: find `= {` after `FIXTURES`, extract the balanced
    /// brace block, then normalize TS-isms to JSON5-ish before
    /// json parsing:
    /// - Convert single quotes to double quotes for keys/strings
    /// - Convert unquoted keys to quoted keys
    /// - Convert regex literals `/pattern/` to string `"pattern"`
    /// - Strip trailing commas
    /// - Strip line + block comments
    ///
    /// This handles the common case without pulling in a full TS
    /// parser. Consumers with exotic TS syntax should codegen JSON
    /// via the documented `gen-json.mjs` script.
    fn load_from_ts_source(text: &str, path: &Path) -> Result<Self, RegistryError> {
        // 1. Strip comments (line + block).
        let cleaned = strip_ts_comments(text);
        // 2. Find `FIXTURES` binding.
        let start = cleaned
            .find("FIXTURES")
            .ok_or_else(|| RegistryError::Malformed {
                path: path.display().to_string(),
                detail: "no `FIXTURES` binding found; use JSON registry or codegen".into(),
            })?;
        let after = &cleaned[start..];
        // 3. Find first `{` after `= `.
        let eq = after.find('=').ok_or_else(|| RegistryError::Malformed {
            path: path.display().to_string(),
            detail: "no `=` after FIXTURES binding".into(),
        })?;
        let brace_start = after[eq..]
            .find('{')
            .ok_or_else(|| RegistryError::Malformed {
                path: path.display().to_string(),
                detail: "no `{` in FIXTURES value".into(),
            })?
            + eq;
        let object_lit = extract_balanced_braces(&after[brace_start..]).ok_or_else(|| {
            RegistryError::Malformed {
                path: path.display().to_string(),
                detail: "unbalanced braces in FIXTURES value".into(),
            }
        })?;
        // 4. TS → JSON-ish massage.
        let json_ish = normalize_ts_object_to_json(&object_lit);
        // 5. Parse. Expected result: { "prime-search-history": {...}, ... }
        let fixtures: BTreeMap<String, FixtureDecl> =
            serde_json::from_str(&json_ish).map_err(|e| RegistryError::Malformed {
                path: path.display().to_string(),
                detail: format!("TS-to-JSON parse: {e}; normalized text: {json_ish}"),
            })?;
        Ok(Self { fixtures })
    }

    pub fn get(&self, id: &str) -> Option<&FixtureDecl> {
        self.fixtures.get(id)
    }

    pub fn require(&self, id: &str) -> Result<&FixtureDecl, RegistryError> {
        self.get(id).ok_or_else(|| RegistryError::NotFound {
            id: id.to_string(),
            known: self.fixtures.keys().cloned().collect(),
        })
    }

    pub fn ids(&self) -> impl Iterator<Item = &String> {
        self.fixtures.keys()
    }

    /// Construct from an in-memory map. Test / library use — the CLI
    /// always goes through [`Self::load`].
    pub fn from_map(fixtures: BTreeMap<String, FixtureDecl>) -> Self {
        Self { fixtures }
    }
}

/// Strip line + block comments from TS source.
fn strip_ts_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut in_string: Option<u8> = None;
    while i < bytes.len() {
        let c = bytes[i];
        // Track string state — comments inside strings are content.
        if let Some(q) = in_string {
            out.push(c as char);
            if c == q && (i == 0 || bytes[i - 1] != b'\\') {
                in_string = None;
            }
            i += 1;
            continue;
        }
        if c == b'"' || c == b'\'' || c == b'`' {
            in_string = Some(c);
            out.push(c as char);
            i += 1;
            continue;
        }
        // Line comment.
        if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // Block comment.
        if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        out.push(c as char);
        i += 1;
    }
    out
}

/// Extract the substring from `s[0..]` that balances the opening `{`.
/// Returns None if braces don't balance.
fn extract_balanced_braces(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut in_string: Option<u8> = None;
    let mut end = 0;
    for (i, &c) in bytes.iter().enumerate() {
        if let Some(q) = in_string {
            if c == q && (i == 0 || bytes[i - 1] != b'\\') {
                in_string = None;
            }
            continue;
        }
        if c == b'"' || c == b'\'' || c == b'`' {
            in_string = Some(c);
            continue;
        }
        if c == b'{' {
            depth += 1;
        } else if c == b'}' {
            depth -= 1;
            if depth == 0 {
                end = i + 1;
                break;
            }
        }
    }
    if end == 0 {
        None
    } else {
        Some(s[..end].to_string())
    }
}

/// Normalize a TS object literal to JSON.
///
/// Transforms applied:
/// - Single-quoted strings → double-quoted
/// - Unquoted object keys → double-quoted
/// - Regex literals `/pattern/flags` → `"pattern"` (keeps the source
///   pattern; flags dropped since Rust regex has different flag
///   syntax anyway)
/// - Trailing commas removed
fn normalize_ts_object_to_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    // Track "expecting a key" state — true after `{` or `,` (ignoring
    // whitespace / newlines).
    let mut expect_key = false;
    while i < bytes.len() {
        let c = bytes[i];
        // Skip whitespace but track for state.
        if c.is_ascii_whitespace() {
            out.push(c as char);
            i += 1;
            continue;
        }
        if c == b'{' || c == b',' {
            out.push(c as char);
            expect_key = c == b'{';
            i += 1;
            // After `,`, next non-ws token could be a key or `}`.
            // Peek: if `}`, we have a trailing comma — strip it.
            let mut j = i;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if c == b',' && j < bytes.len() && bytes[j] == b'}' {
                // Trailing comma — remove the just-written `,`.
                out.pop();
                i = j;
                continue;
            }
            if c == b',' {
                expect_key = true;
            }
            continue;
        }
        // Single-quoted string → double-quoted.
        if c == b'\'' {
            out.push('"');
            i += 1;
            while i < bytes.len() && bytes[i] != b'\'' {
                if bytes[i] == b'"' {
                    out.push_str("\\\"");
                } else {
                    out.push(bytes[i] as char);
                }
                i += 1;
            }
            out.push('"');
            i += 1; // closing quote
            expect_key = false;
            continue;
        }
        // Regex literal /pattern/flags. Only recognize when in value
        // position (not expecting a key).
        if c == b'/'
            && !expect_key
            && i + 1 < bytes.len()
            && bytes[i + 1] != b'/'
            && bytes[i + 1] != b'*'
        {
            i += 1;
            let mut pat = String::new();
            while i < bytes.len() && bytes[i] != b'/' {
                // Regex source escapes: `\[`, `\d`, etc. In the JSON
                // output these must be double-escaped so the result
                // is a valid JSON string that, when parsed, yields
                // the regex source verbatim.
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    // Preserve the backslash for regex semantics but
                    // escape it for JSON.
                    pat.push('\\');
                    pat.push('\\');
                    pat.push(bytes[i + 1] as char);
                    i += 2;
                    continue;
                }
                if bytes[i] == b'"' {
                    pat.push_str("\\\"");
                    i += 1;
                    continue;
                }
                pat.push(bytes[i] as char);
                i += 1;
            }
            i += 1; // closing /
            // Skip flags (g / i / m / s etc.)
            while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                i += 1;
            }
            out.push('"');
            out.push_str(&pat);
            out.push('"');
            expect_key = false;
            continue;
        }
        // Unquoted key. Alphanumeric / underscore / dash / dot until `:`.
        if expect_key && (c.is_ascii_alphanumeric() || c == b'_' || c == b'-' || c == b'.') {
            let mut key = String::new();
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric()
                    || bytes[i] == b'_'
                    || bytes[i] == b'-'
                    || bytes[i] == b'.')
            {
                key.push(bytes[i] as char);
                i += 1;
            }
            out.push('"');
            out.push_str(&key);
            out.push('"');
            expect_key = false;
            continue;
        }
        // Double-quoted string — pass through.
        if c == b'"' {
            out.push('"');
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    out.push(bytes[i] as char);
                    out.push(bytes[i + 1] as char);
                    i += 2;
                    continue;
                }
                out.push(bytes[i] as char);
                i += 1;
            }
            out.push('"');
            i += 1;
            expect_key = false;
            continue;
        }
        if c == b':' {
            out.push(':');
            i += 1;
            expect_key = false;
            continue;
        }
        // Colon or other punct — pass through.
        out.push(c as char);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write(tmp: &TempDir, name: &str, content: &str) -> std::path::PathBuf {
        let p = tmp.path().join(name);
        std::fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn load_single_fixture() {
        let tmp = TempDir::new().unwrap();
        let path = write(
            &tmp,
            "reg.json",
            r#"{
                "version": 1,
                "fixtures": {
                    "prime-search-history": {
                        "testID": "qa-chip-prime-search-history",
                        "signal": {
                            "regex": "\\[fixture\\] prime-search-history: seeded (\\d+) rows",
                            "level": "log"
                        },
                        "timeoutMs": 8000
                    }
                }
            }"#,
        );
        let reg = FixtureRegistry::load(&path).unwrap();
        let f = reg.require("prime-search-history").unwrap();
        assert_eq!(f.test_id, "qa-chip-prime-search-history");
        assert_eq!(f.timeout_ms, 8000);
        assert_eq!(f.signal.level.as_deref(), Some("log"));
    }

    #[test]
    fn not_found_lists_available() {
        let tmp = TempDir::new().unwrap();
        let path = write(
            &tmp,
            "reg.json",
            r#"{"fixtures":{
                "a":{"testID":"t-a","signal":{"regex":"a"},"timeoutMs":100},
                "b":{"testID":"t-b","signal":{"regex":"b"},"timeoutMs":100}
            }}"#,
        );
        let reg = FixtureRegistry::load(&path).unwrap();
        let err = reg.require("nope").unwrap_err();
        match err {
            RegistryError::NotFound { known, .. } => {
                assert_eq!(known, vec!["a".to_string(), "b".to_string()]);
            }
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn malformed_json_surfaces_error() {
        let tmp = TempDir::new().unwrap();
        let path = write(&tmp, "reg.json", "not json");
        let err = FixtureRegistry::load(&path).unwrap_err();
        matches!(err, RegistryError::Malformed { .. });
    }

    #[test]
    fn multi_fixture_ids_iterate_deterministic() {
        let tmp = TempDir::new().unwrap();
        let path = write(
            &tmp,
            "reg.json",
            r#"{"fixtures":{
                "z":{"testID":"z","signal":{"regex":"."},"timeoutMs":100},
                "a":{"testID":"a","signal":{"regex":"."},"timeoutMs":100},
                "m":{"testID":"m","signal":{"regex":"."},"timeoutMs":100}
            }}"#,
        );
        let reg = FixtureRegistry::load(&path).unwrap();
        let ids: Vec<&String> = reg.ids().collect();
        assert_eq!(ids, vec!["a", "m", "z"]);
    }
}

#[cfg(test)]
mod ts_reader_tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_ts(tmp: &TempDir, name: &str, content: &str) -> std::path::PathBuf {
        let p = tmp.path().join(name);
        std::fs::File::create(&p)
            .unwrap()
            .write_all(content.as_bytes())
            .unwrap();
        p
    }

    #[test]
    fn ts_registry_basic() {
        let tmp = TempDir::new().unwrap();
        let path = write_ts(
            &tmp,
            "registry.ts",
            r#"
export const FIXTURES: Record<string, any> = {
  'prime-search-history': {
    testID: 'qa-chip-prime-search-history',
    signal: { regex: /\[fixture\] prime-search-history: seeded (\d+) rows/, level: 'log' },
    timeoutMs: 8000,
  },
};
"#,
        );
        let reg = FixtureRegistry::load(&path).unwrap();
        let f = reg.get("prime-search-history").unwrap();
        assert_eq!(f.test_id, "qa-chip-prime-search-history");
        assert_eq!(f.timeout_ms, 8000);
        assert!(f.signal.regex.contains("fixture"));
    }

    #[test]
    fn ts_registry_multi_fixtures() {
        let tmp = TempDir::new().unwrap();
        let path = write_ts(
            &tmp,
            "registry.ts",
            r#"
export const FIXTURES = {
  'foo': { testID: 't-foo', signal: { regex: 'foo-ready' }, timeoutMs: 5000 },
  'bar': { testID: 't-bar', signal: { regex: 'bar-ready' }, timeoutMs: 3000 },
};
"#,
        );
        let reg = FixtureRegistry::load(&path).unwrap();
        assert!(reg.get("foo").is_some());
        assert!(reg.get("bar").is_some());
    }
}
