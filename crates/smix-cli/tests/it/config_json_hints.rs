//! v2 break #3, decision (a): `.smix/config.yaml` is the one config file
//! smix reads. `.smix/config.json` is never loaded — every past mention
//! was a hint/doc string. This gate keeps shipped `.rs` sources (outside
//! `tests/`) free of the stale `.smix/config.json` filename so the hints
//! point at the real file.

use std::fs;
use std::path::{Path, PathBuf};

fn workspace_crates_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR = <root>/crates/smix-cli → up two = <root>.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .join("crates")
}

fn collect_shipped_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read_dir").flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            // Skip test trees (allowed to name the legacy file) and build
            // output.
            if name == "tests" || name == "target" {
                continue;
            }
            collect_shipped_rs(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_config_json_hints_in_shipped_sources() {
    let mut files = Vec::new();
    collect_shipped_rs(&workspace_crates_dir(), &mut files);
    let offenders: Vec<String> = files
        .into_iter()
        .filter(|p| {
            fs::read_to_string(p)
                .map(|s| s.contains(".smix/config.json"))
                .unwrap_or(false)
        })
        .map(|p| p.display().to_string())
        .collect();
    assert!(
        offenders.is_empty(),
        "shipped sources must not mention `.smix/config.json` (repoint to .yaml): {offenders:?}"
    );
}
