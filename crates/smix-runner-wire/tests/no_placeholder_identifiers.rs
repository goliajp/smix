//! Documentation placeholders must not be load-bearing.
//!
//! `com.example.app` is the bundle id the README and the guides use in
//! examples. Twice now it has ended up in code that runs: once as the
//! CLI's default bundle, where it meant `smix run flow.yaml` could not
//! drive a real app at all, and once inside the Android runner's id
//! lookup, where it was one of three resource-id spellings tried and so
//! the qualified spelling only ever matched a reader who had copied the
//! example verbatim.
//!
//! Neither failed anything. A placeholder that stands in for a real
//! value produces working-looking code that is inert for everyone but
//! the person who wrote the example — which is why this is checked
//! rather than remembered.
//!
//! Examples, tests and fixtures may use it freely; that is what it is
//! for. Only the sources that run against a user's app may not.

use std::path::{Path, PathBuf};

/// Strings that only ever name something imaginary.
const PLACEHOLDERS: &[&str] = &["com.example.app", "com.example.myapp"];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn collect(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let name = p.file_name().unwrap_or_default().to_string_lossy();
        // Build output is generated, and test trees are allowed the
        // placeholder — asserting on it is a legitimate use.
        if p.is_dir() {
            // `androidTest` is deliberately absent: that is where the
            // Android runner itself lives, instrumentation-hosted.
            const NOT_PRODUCTION: &[&str] = &[
                "build",
                "target",
                "tests",
                "test",
                "benches",
                "fuzz",
                "node_modules",
            ];
            if NOT_PRODUCTION.contains(&name.as_ref()) {
                continue;
            }
            collect(&p, ext, out);
        } else if p.extension().is_some_and(|e| e == ext) {
            out.push(p);
        }
    }
}

/// Files under `src/` that are nonetheless test code.
///
/// A whole file can be a test module: `#[cfg(test)] mod guide_gate;`
/// in `main.rs` puts every line of `guide_gate.rs` behind the same
/// gate as an inline `#[cfg(test)] mod tests { … }`, and a bin crate
/// with no lib target has nowhere else to put a test that needs its
/// private items. The brace-depth skip below cannot see this, because
/// the marker is in the parent file rather than in the file it
/// exempts. Collected by reading those declarations rather than by
/// listing filenames, so a second one needs nothing here.
fn whole_file_test_modules(sources: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for path in sources {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let dir = match path.parent() {
            Some(d) => d,
            None => continue,
        };
        let mut armed = false;
        for line in text.lines() {
            let t = line.trim();
            if t.starts_with("#[cfg(test)]") {
                // `#[cfg(test)] mod x;` on one line, or on the next.
                if let Some(name) = t.strip_prefix("#[cfg(test)]").map(str::trim)
                    && let Some(m) = module_decl(name)
                {
                    out.push(dir.join(format!("{m}.rs")));
                    out.push(dir.join(m).join("mod.rs"));
                    continue;
                }
                armed = true;
                continue;
            }
            if armed {
                if let Some(m) = module_decl(t) {
                    out.push(dir.join(format!("{m}.rs")));
                    out.push(dir.join(m).join("mod.rs"));
                }
                armed = false;
            }
        }
    }
    out
}

/// `mod name;` → `name`. A `mod name {` is an inline module, which the
/// brace-depth skip already handles.
fn module_decl(line: &str) -> Option<&str> {
    line.strip_prefix("mod ")
        .and_then(|rest| rest.strip_suffix(';'))
        .map(str::trim)
        .filter(|n| !n.is_empty() && n.chars().all(|c| c.is_alphanumeric() || c == '_'))
}

/// Kotlin and Rust sources that drive a real device.
fn production_sources(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect(&root.join("crates"), "rs", &mut out);
    collect(&root.join("android-runner/app/src"), "kt", &mut out);
    let test_modules = whole_file_test_modules(&out);
    out.retain(|p| !test_modules.contains(p));
    out
}

#[test]
fn no_production_source_treats_a_placeholder_as_a_real_bundle_id() {
    let root = workspace_root();
    let sources = production_sources(&root);
    assert!(
        sources.len() >= 40,
        "found only {} sources — the walk stopped matching and this \
         check would pass by knowing nothing",
        sources.len()
    );

    let mut offenders: Vec<String> = Vec::new();
    for path in &sources {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        // `#[cfg(test)]` modules live inside source files and are
        // tests like any other, so their span is skipped by brace
        // depth. The first version of this check tried to do that by
        // exempting lines starting with `#[` or `assert`, and reported
        // fourteen findings that were all fixture data — a gate at the
        // wrong granularity is as useless as no gate, just louder.
        let mut test_span_until: Option<i32> = None;
        let mut depth: i32 = 0;

        for (lineno, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();

            if trimmed.starts_with("#[cfg(test)]") {
                test_span_until = Some(depth);
            }
            depth += line.matches('{').count() as i32;
            depth -= line.matches('}').count() as i32;
            if let Some(exit_depth) = test_span_until {
                if depth <= exit_depth && trimmed.starts_with('}') {
                    test_span_until = None;
                }
                continue;
            }

            // Prose about the placeholder is the point of this file's
            // own doc comment, and of the comments left where it used
            // to live.
            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                continue;
            }
            // Showing the reader an example command is the placeholder
            // doing its job. `smix runner up <device> --bundle
            // com.example.app` inside an error message is how someone
            // learns the flag's shape.
            if line.contains("--bundle ") || line.contains("e.g.") {
                continue;
            }
            for p in PLACEHOLDERS {
                if line.contains(p) {
                    offenders.push(format!(
                        "{}:{}: `{p}` is being used as a real bundle id\n      {}",
                        path.strip_prefix(&root).unwrap_or(path).display(),
                        lineno + 1,
                        line.trim()
                    ));
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "a documentation placeholder is load-bearing in code that runs \
         against a user's app:\n  {}\nUse the value the caller supplied \
         — for the runners that is the `App-Bundle-Id` header.",
        offenders.join("\n  ")
    );
}
