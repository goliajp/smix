//! What an app owes, with an id, so coverage can be reconciled.
//!
//! The requirement is usually already written down — in the comment
//! above a flow, in a ticket, in a design note. What it does not have
//! is an identity, and without one nothing can answer "is this covered
//! on both platforms" except a person reading two test suites side by
//! side and remembering.
//!
//! This crate is the identity and the arithmetic. It reads contracts
//! and per-platform claims and answers three sets: nobody claims,
//! one platform claims, both claim.
//!
//! **It reports who claimed, never who verified.** Those are different
//! words. A claim says a test suite means to cover a requirement; it
//! does not say the test is good, or that it passed, or that the two
//! platforms' tests check the same thing — which is not mechanically
//! decidable, and pretending otherwise would make this one more cheap
//! signal standing in for the thing to be proven. The output uses the
//! word it can support.

#![forbid(unsafe_code)]

use serde::Deserialize;
use std::fmt;

/// One requirement the app owes, and the id that lets it be claimed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contract {
    pub id: String,
    pub statement: String,
    /// Where this was read from, carried so a refusal can say where.
    pub origin: String,
}

/// Why a contract file was refused.
///
/// Each variant names the thing that is wrong and where it was read,
/// because a parse error that says only "invalid" sends the reader
/// back to the file to do the search again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractError {
    /// The file is not the shape a contract file has.
    Malformed { origin: String, detail: String },
    /// An entry is missing a field it cannot do without.
    MissingField {
        origin: String,
        field: &'static str,
        entry: usize,
    },
    /// An entry has the field and it is empty, which is the same
    /// absence wearing a present field.
    BlankField {
        origin: String,
        field: &'static str,
        entry: usize,
    },
    /// One id, two requirements. Every later answer about that id is
    /// meaningless, so this is refused rather than warned about.
    DuplicateId {
        origin: String,
        id: String,
        first: usize,
        second: usize,
    },
    /// A claim names an id no contract carries. Silently dropping it
    /// would leave a requirement looking unclaimed while somebody
    /// believes they are covering it.
    UnknownContract { origin: String, id: String },
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed { origin, detail } => write!(
                f,
                "{origin}: not a contract file — a contract file is a list of \
                 entries, each with an id and a statement ({detail})"
            ),
            Self::MissingField {
                origin,
                field,
                entry,
            } => write!(
                f,
                "{origin}: entry {entry} has no `{field}`. A contract without an \
                 id cannot be claimed by anything, and one without a statement \
                 stands for nothing"
            ),
            Self::BlankField {
                origin,
                field,
                entry,
            } => write!(
                f,
                "{origin}: entry {entry} has an empty `{field}`. A present-but-empty \
                 field passes a check for the field and says nothing"
            ),
            Self::DuplicateId {
                origin,
                id,
                first,
                second,
            } => write!(
                f,
                "{origin}: id `{id}` is on entry {first} and entry {second}. One id \
                 pointing at two requirements makes every claim on it ambiguous — \
                 there is no answer to which of the two a claim covers"
            ),
            Self::UnknownContract { origin, id } => write!(
                f,
                "{origin}: claims `{id}`, which no contract carries. A mistyped id \
                 leaves the requirement it meant looking unclaimed while somebody \
                 believes they are covering it"
            ),
        }
    }
}

impl std::error::Error for ContractError {}

#[derive(Deserialize)]
struct RawContract {
    id: Option<String>,
    statement: Option<String>,
}

/// Read a contract file.
///
/// `origin` is what the file should be called in any refusal — a path,
/// usually. It is a parameter rather than read from the text because
/// the caller knows where the bytes came from and this does not.
pub fn parse_contracts(text: &str, origin: &str) -> Result<Vec<Contract>, ContractError> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let raw: Vec<RawContract> =
        serde_norway::from_str(text).map_err(|e| ContractError::Malformed {
            origin: origin.to_string(),
            detail: e.to_string(),
        })?;

    let mut out: Vec<Contract> = Vec::with_capacity(raw.len());
    let mut seen: Vec<(String, usize)> = Vec::with_capacity(raw.len());

    for (i, entry) in raw.into_iter().enumerate() {
        let n = i + 1;
        let id = required(entry.id, "id", origin, n)?;
        let statement = required(entry.statement, "statement", origin, n)?;
        if let Some((_, first)) = seen.iter().find(|(seen_id, _)| *seen_id == id) {
            return Err(ContractError::DuplicateId {
                origin: origin.to_string(),
                id,
                first: *first,
                second: n,
            });
        }
        seen.push((id.clone(), n));
        out.push(Contract {
            id,
            statement,
            origin: origin.to_string(),
        });
    }
    Ok(out)
}

fn required(
    value: Option<String>,
    field: &'static str,
    origin: &str,
    entry: usize,
) -> Result<String, ContractError> {
    match value {
        None => Err(ContractError::MissingField {
            origin: origin.to_string(),
            field,
            entry,
        }),
        Some(v) if v.trim().is_empty() => Err(ContractError::BlankField {
            origin: origin.to_string(),
            field,
            entry,
        }),
        Some(v) => Ok(v),
    }
}
