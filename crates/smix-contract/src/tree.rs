//! Reconciling a whole tree, in every notation it holds at once.
//!
//! A team has all three during the years it takes to move from one to
//! another: contracts in a file, claims in a file, claims in the source
//! beside the tests. One answer comes out of the three.

use crate::{
    Claim, Contract, ContractError, Reconciliation, parse_claims, parse_contracts, reconcile,
    scan_claims,
};
use std::path::{Path, PathBuf};

/// Directories whose contents are copies, generated, or not source.
///
/// `target/` holds copies of source and of generated files; scanning it
/// would double every claim and read platforms out of path fragments
/// that mean something else.
const NOT_SOURCE: [&str; 5] = ["target", ".git", "node_modules", "build", ".build"];

/// Extensions worth reading for `// covers:` lines.
///
/// Named rather than "everything that is not binary": a scanner that
/// reads whatever it finds will one day read a fixture describing the
/// scanner and claim what the fixture is about.
const SOURCE_EXTS: [&str; 6] = ["swift", "kt", "kts", "rs", "java", "m"];

/// Reconcile every contract, claim file and source claim under `root`.
pub fn reconcile_tree(root: &Path, expected: &[&str]) -> Result<Reconciliation, ContractError> {
    let mut contracts: Vec<Contract> = Vec::new();
    let mut claims: Vec<Claim> = Vec::new();

    for path in walk(root) {
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();
        let name = path.file_name().map(|n| n.to_string_lossy().to_string());
        let Some(name) = name else { continue };
        let Ok(body) = std::fs::read_to_string(&path) else {
            continue;
        };

        if name.ends_with(".contracts.yaml") {
            for c in parse_contracts(&body, &rel)? {
                if let Some(first) = contracts.iter().find(|p| p.id == c.id) {
                    return Err(ContractError::DuplicateAcrossFiles {
                        id: c.id,
                        first: first.origin.clone(),
                        second: rel,
                    });
                }
                contracts.push(c);
            }
        } else if name.ends_with(".claims.yaml") {
            claims.extend(parse_claims(&body, &rel)?);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| SOURCE_EXTS.contains(&e))
        {
            // Only pay for the platform when the file actually claims
            // something: most source files do not, and refusing them
            // all for living outside a platform directory would make
            // this unusable.
            let unplaced = scan_claims(&body, &rel, "");
            if unplaced.is_empty() {
                continue;
            }
            let platform =
                platform_of(&rel, expected).ok_or_else(|| ContractError::PlatformNotInPath {
                    origin: rel.clone(),
                    expected: expected.iter().map(|p| (*p).to_string()).collect(),
                })?;
            claims.extend(scan_claims(&body, &rel, platform));
        }
    }

    reconcile(&contracts, &claims, expected)
}

/// Which platform a path belongs to, by the directories it sits under.
///
/// Read rather than declared: nobody keeps a list of which directories
/// are which in step with the directories. Guessing is worse than
/// refusing — guess wrong and a requirement covered on one platform
/// reads as covered on both, which is this layer's whole subject
/// inverted.
pub fn platform_of<'a>(rel: &str, expected: &[&'a str]) -> Option<&'a str> {
    let parts: Vec<&str> = rel.split(['/', '\\']).collect();
    expected
        .iter()
        .copied()
        .find(|p| parts.iter().any(|seg| seg.eq_ignore_ascii_case(p)))
}

fn walk(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if !NOT_SOURCE.contains(&name.as_str()) {
                    stack.push(path);
                }
            } else {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}
