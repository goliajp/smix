//! Joining claims to contracts, and refusing to call an empty corpus agreement.

use crate::{Contract, ContractError};

/// One platform's statement that it means to cover a contract.
///
/// The platform is a name rather than an enum. Three enums for this
/// already exist in this workspace — the driver's, the capsule's, and
/// the adapter's — and a leaf parser pulling in a heavy crate to reach
/// one of them, or minting a fourth, are both worse than carrying the
/// name and refusing the ones nobody expects. The refusal is what
/// makes the name safe: a claim naming `androd` is an error here, the
/// same way a mistyped contract id is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    pub contract_id: String,
    pub platform: String,
    pub origin: String,
}

/// A contract some but not all expected platforms claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartialClaim {
    pub contract: Contract,
    /// The expected platforms that claimed it, in the expected order.
    pub claimed_by: Vec<String>,
    /// The expected platforms that did not. This is the part a reader
    /// acts on, so it is carried rather than left to be worked out.
    pub missing: Vec<String>,
}

/// Who claimed what.
///
/// Three sets, and the word is `claimed` in all of them. Nothing here
/// says a claimed requirement is verified: a claim says a suite means
/// to cover it, not that the test is good, that it passed, or that two
/// platforms' tests check the same thing — the last of which is not
/// mechanically decidable. Reporting it as coverage would make this
/// one more cheap signal standing in for the thing to be proven.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reconciliation {
    pub unclaimed: Vec<Contract>,
    pub partially_claimed: Vec<PartialClaim>,
    pub fully_claimed: Vec<Contract>,
}

/// Join claims to contracts against the platforms expected to cover them.
///
/// Refuses an empty comparison. Three empty sets over an empty corpus
/// is the shape of perfect coverage, and so is every contract being
/// trivially "fully claimed" when nothing is expected — both are
/// agreements nobody earned. A predicate that holds on empty input is
/// not a predicate.
pub fn reconcile(
    contracts: &[Contract],
    claims: &[Claim],
    expected: &[&str],
) -> Result<Reconciliation, ContractError> {
    if contracts.is_empty() {
        return Err(ContractError::NothingToReconcile {
            detail: "no contracts — an empty corpus agrees with everything".into(),
        });
    }
    if expected.is_empty() {
        return Err(ContractError::NothingToReconcile {
            detail: "no expected platforms — with nothing expected every contract is \
                     trivially covered"
                .into(),
        });
    }

    for claim in claims {
        if !contracts.iter().any(|c| c.id == claim.contract_id) {
            return Err(ContractError::UnknownContract {
                origin: claim.origin.clone(),
                id: claim.contract_id.clone(),
            });
        }
        if !expected.contains(&claim.platform.as_str()) {
            return Err(ContractError::UnexpectedPlatform {
                origin: claim.origin.clone(),
                platform: claim.platform.clone(),
                expected: expected.iter().map(|p| (*p).to_string()).collect(),
            });
        }
    }

    let mut out = Reconciliation {
        unclaimed: Vec::new(),
        partially_claimed: Vec::new(),
        fully_claimed: Vec::new(),
    };

    for contract in contracts {
        // Deduplicated on the way in: two claims from one platform are
        // one platform. Counting a repeated reading as corroboration is
        // the arithmetic form of the mistake this crate is about.
        let claimed_by: Vec<String> = expected
            .iter()
            .filter(|p| {
                claims
                    .iter()
                    .any(|c| c.contract_id == contract.id && c.platform == **p)
            })
            .map(|p| (*p).to_string())
            .collect();
        let missing: Vec<String> = expected
            .iter()
            .filter(|p| !claimed_by.iter().any(|c| c == *p))
            .map(|p| (*p).to_string())
            .collect();

        if claimed_by.is_empty() {
            out.unclaimed.push(contract.clone());
        } else if missing.is_empty() {
            out.fully_claimed.push(contract.clone());
        } else {
            out.partially_claimed.push(PartialClaim {
                contract: contract.clone(),
                claimed_by,
                missing,
            });
        }
    }
    Ok(out)
}
