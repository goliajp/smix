//! Every alias is answered through the one function that refuses a
//! disagreement between this machine's registry and a checkout's book.
//!
//! `SimRegistry::resolve` answers from the merged view and knows nothing
//! about where a row came from; `MergedRegistry::resolve_ref` refuses an
//! alias the two books give to different devices (§9 #9). A caller that
//! reaches for the first skips the refusal without a word, so it is
//! looked for rather than trusted.

use std::path::Path;

fn rust_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn no_caller_resolves_an_alias_past_the_disagreement_check() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut files = Vec::new();
    for c in std::fs::read_dir(crates).unwrap() {
        let src = c.unwrap().path().join("src");
        if src.is_dir() {
            rust_files(&src, &mut files);
        }
    }
    // The registry defines the method; everyone else must not call it.
    files.retain(|p| !p.ends_with("smix-simctl/src/registry.rs"));
    assert!(
        files.len() >= 100,
        "scanned only {} files — the walk stopped finding the workspace, and a \
         scan of nothing finds no bypass",
        files.len()
    );
    let bypasses: Vec<String> = files
        .iter()
        .filter_map(|p| {
            let text = std::fs::read_to_string(p).ok()?;
            text.lines()
                .enumerate()
                .find(|(_, l)| l.contains(".registry.resolve("))
                .map(|(i, _)| format!("{}:{}", p.display(), i + 1))
        })
        .collect();
    assert!(
        bypasses.is_empty(),
        "these resolve an alias without the refusal a disagreeing checkout book \
         must trigger — use MergedRegistry::resolve_ref: {bypasses:?}"
    );
}
