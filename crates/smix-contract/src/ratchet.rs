//! Forbidding the loss of coverage, without demanding its increase.
//!
//! A coverage target is met by writing claims. A ratchet cannot be:
//! there is nothing to write that turns a lost platform back into a
//! covered one except covering it again.
//!
//! The baseline lists ids rather than counts. A number going down says
//! something went; a name going missing says what — and it shows up in
//! a diff as a line with a name on it, which is what makes
//! "regenerate the baseline until it is green" a visible act rather
//! than a quiet one.

use crate::Reconciliation;
use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

/// What was covered, by whom, at the point this was written down.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Baseline {
    /// contract id → the platforms that claimed it, sorted.
    covered: BTreeMap<String, Vec<String>>,
}

impl Baseline {
    /// The platforms a contract was claimed by, if it was known then.
    pub fn platforms(&self, id: &str) -> Option<&[String]> {
        self.covered.get(id).map(Vec::as_slice)
    }

    pub fn is_empty(&self) -> bool {
        self.covered.is_empty()
    }
}

/// Write down what is covered now.
///
/// `expected` is passed rather than inferred. The first version read
/// it off whichever contract happened to be partially claimed — and
/// when every contract was covered on both platforms there was no
/// partial one to read, so it recorded a placeholder that compared
/// equal to everything. The ratchet could not report a loss on exactly
/// the corpus a team wants it for: the one where nothing is missing
/// yet.
pub fn baseline_of(r: &Reconciliation, expected: &[&str]) -> Baseline {
    let mut covered: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for c in &r.fully_claimed {
        covered.insert(
            c.id.clone(),
            expected.iter().map(|p| (*p).to_string()).collect(),
        );
    }
    for p in &r.partially_claimed {
        covered.insert(p.contract.id.clone(), p.claimed_by.clone());
    }
    Baseline { covered }
}

impl fmt::Display for Baseline {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (id, platforms) in &self.covered {
            writeln!(f, "{id}: {}", platforms.join(", "))?;
        }
        Ok(())
    }
}

impl FromStr for Baseline {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut covered = BTreeMap::new();
        for (i, line) in s.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (id, rest) = line
                .split_once(':')
                .ok_or_else(|| format!("line {}: no `id: platforms`", i + 1))?;
            covered.insert(
                id.trim().to_string(),
                rest.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect(),
            );
        }
        Ok(Baseline { covered })
    }
}

/// Something that was covered and is not any more.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Regression {
    /// A platform that used to claim it does not.
    PlatformLost { id: String, lost: Vec<String> },
    /// The contract itself is gone from the corpus. Deleting a
    /// requirement is legitimate; doing it silently is not.
    ContractGone { id: String },
}

impl Regression {
    /// Should this stop a build?
    ///
    /// Losing coverage should. A deleted requirement should be said
    /// and not blocked — the reader decides whether the deletion was
    /// meant, and a gate that refuses deletions makes the corpus
    /// grow-only.
    pub fn blocks(&self) -> bool {
        matches!(self, Self::PlatformLost { .. })
    }
}

impl fmt::Display for Regression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PlatformLost { id, lost } => write!(
                f,
                "{id} was claimed by {} and is not any more. Coverage that existed \
                 has gone; nothing here asks for more of it, only that what is \
                 there stays.",
                lost.join(", ")
            ),
            Self::ContractGone { id } => write!(
                f,
                "{id} was in the baseline and is not in the corpus. Deleting a \
                 requirement is a legitimate act and this does not block it — but \
                 it should not happen quietly."
            ),
        }
    }
}

/// What has been lost since the baseline was written.
pub fn regressions(base: &Baseline, now: &Reconciliation, expected: &[&str]) -> Vec<Regression> {
    let mut out = Vec::new();
    let mut present: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for c in &now.fully_claimed {
        present.insert(
            c.id.as_str(),
            expected.iter().map(|p| (*p).to_string()).collect(),
        );
    }
    for p in &now.partially_claimed {
        present.insert(p.contract.id.as_str(), p.claimed_by.clone());
    }
    for c in &now.unclaimed {
        present.insert(c.id.as_str(), Vec::new());
    }

    for (id, was) in &base.covered {
        let Some(is) = present.get(id.as_str()) else {
            out.push(Regression::ContractGone { id: id.clone() });
            continue;
        };
        let lost: Vec<String> = was.iter().filter(|p| !is.contains(p)).cloned().collect();
        if !lost.is_empty() {
            out.push(Regression::PlatformLost {
                id: id.clone(),
                lost,
            });
        }
    }
    out
}
