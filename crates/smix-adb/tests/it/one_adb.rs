//! Every adb smix runs is the one `smix_adb::adb_path` resolves.
//!
//! A dozen call sites each started the first `adb` on `PATH`, and on a Mac
//! driven over ssh that was a quarantined Homebrew binary that never got
//! past its first launch. One literal `Command::new("adb")` brings that
//! back for its caller, so it is looked for rather than trusted.

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
fn no_caller_starts_an_adb_by_name() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut files = Vec::new();
    for c in std::fs::read_dir(crates).unwrap() {
        let src = c.unwrap().path().join("src");
        if src.is_dir() {
            rust_files(&src, &mut files);
        }
    }
    assert!(
        files.len() >= 100,
        "scanned only {} files — the walk stopped finding the workspace, and a \
         scan of nothing finds no stray adb",
        files.len()
    );
    let needle = concat!("new(", "\"adb\")");
    let found: Vec<String> = files
        .iter()
        .filter(|p| std::fs::read_to_string(p).unwrap().contains(needle))
        .map(|p| p.display().to_string())
        .collect();
    assert!(
        found.is_empty(),
        "these start the first adb on PATH instead of smix_adb::adb_path: {found:?}"
    );
}
