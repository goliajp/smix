#![no_main]
//! Any bytes fed to the contract and claim parsers must refuse or
//! return, never panic.
//!
//! A contract file is written by a person or by an agent and read from
//! disk, so its content is not something this crate gets to assume
//! anything about. Refusing malformed input is the crate's entire job
//! on the way in; panicking is the one answer it must never give,
//! because a panic in a coverage tool takes the whole reconciliation
//! down and says nothing about which file was wrong.

use libfuzzer_sys::fuzz_target;
use smix_contract::{parse_claims, parse_contracts};

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    // Both parsers, same bytes: a claim file and a contract file are
    // different shapes of the same YAML, and either can be handed the
    // other by mistake.
    if let Ok(contracts) = parse_contracts(text, "fuzz") {
        // Whatever came back must be self-consistent: parsing promises
        // no duplicate ids, and every later answer rests on that.
        for (i, c) in contracts.iter().enumerate() {
            assert!(!c.id.trim().is_empty());
            assert!(!c.statement.trim().is_empty());
            assert!(
                !contracts[..i].iter().any(|prev| prev.id == c.id),
                "parse_contracts returned a duplicate id, which it refuses"
            );
        }
    }
    if let Ok(claims) = parse_claims(text, "fuzz") {
        for c in &claims {
            assert!(!c.contract_id.trim().is_empty());
            assert!(!c.platform.trim().is_empty());
        }
    }
});
