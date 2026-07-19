//! The wire's version is not the runner's version.
//!
//! Tying them together made every version difference fatal: a runner up
//! since before a CLI upgrade was killed and the user told to reinstall,
//! even when the wire between them had not moved. Negotiation asks the
//! question that matters — do we speak the same shape — and leaves the
//! semvers to say what they are for.

use smix_runner_wire::{WIRE_SCHEMA_SUPPORTED, WireSchemaInfo, negotiate_wire_schema};

#[test]
fn both_ends_settle_on_the_newest_they_share() {
    assert_eq!(negotiate_wire_schema(&[1, 2], &[1, 2]), Some(2));
    assert_eq!(negotiate_wire_schema(&[1, 2], &[1]), Some(1));
    assert_eq!(negotiate_wire_schema(&[1], &[1, 2]), Some(1));
}

/// The case the old exact-match check could not express: different builds,
/// same wire.
#[test]
fn a_newer_client_still_talks_to_an_older_runner() {
    assert_eq!(negotiate_wire_schema(&[1, 2, 3], &[1, 2]), Some(2));
}

#[test]
fn no_shared_schema_is_none_rather_than_a_guess() {
    assert_eq!(negotiate_wire_schema(&[3], &[1, 2]), None);
    assert_eq!(negotiate_wire_schema(&[], &[1, 2]), None);
    assert_eq!(negotiate_wire_schema(&[1, 2], &[]), None);
}

/// A runner too old to have been asked reports nothing, and that is not the
/// same as a runner that shares no schema. One is silence, the other is a
/// refusal.
#[test]
fn silence_is_not_refusal() {
    let old: WireSchemaInfo = serde_json::from_str("{}").expect("absent fields default");
    assert!(old.supports.is_empty());
    assert_eq!(old.negotiated, None);
}

#[test]
fn this_build_speaks_the_schema_it_ships() {
    assert!(!WIRE_SCHEMA_SUPPORTED.is_empty());
    assert_eq!(
        negotiate_wire_schema(WIRE_SCHEMA_SUPPORTED, WIRE_SCHEMA_SUPPORTED),
        Some(*WIRE_SCHEMA_SUPPORTED.iter().max().expect("non-empty")),
        "a build has to agree with itself"
    );
}

#[test]
fn the_info_rides_the_health_body_as_camel_case() {
    let info = WireSchemaInfo {
        supports: vec![1, 2],
        negotiated: Some(2),
    };
    let json = serde_json::to_string(&info).expect("serializes");
    assert_eq!(json, r#"{"supports":[1,2],"negotiated":2}"#);
}

/// The runner ships inside the CLI, so the schema list exists twice — once
/// in Rust, once in the Swift that answers /health. Two copies of one fact
/// is how three SDKs came to speak a wire no runner served, so this reads
/// the Swift and refuses to let them differ.
#[test]
fn the_runner_and_the_wire_crate_claim_the_same_schemas() {
    let swift = include_str!("../../../swift-bridge/Sources/SmixRunnerCore/HealthRoute.swift");
    // The type annotation is a bracketed list too, so match the assignment.
    let (_, rhs) = swift
        .lines()
        .find_map(|l| l.split_once("wireSchemaSupported: [UInt32] = "))
        .expect("HealthRoute stopped declaring wireSchemaSupported — this check reads it by name");
    let declared: Vec<u32> = rhs
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .filter_map(|n| n.trim().parse().ok())
        .collect();
    assert!(
        !declared.is_empty(),
        "read no schemas out of `{rhs}` — the declaration's shape changed and this \
         check would pass by knowing nothing"
    );
    assert_eq!(
        declared, WIRE_SCHEMA_SUPPORTED,
        "the runner says it speaks {declared:?} and this crate says {WIRE_SCHEMA_SUPPORTED:?}"
    );
}

/// The README's stability section is a promise to users about this
/// crate's constant, so it is held to it.
///
/// `WIRE_SCHEMA_SUPPORTED` says "add a version here when the shape
/// changes". The README states the list as prose — "currently `[1, 2]`"
/// — and the day schema 3 lands, that sentence becomes a promise the
/// runner no longer keeps, in the one section a reader consults before
/// committing to a major version.
#[test]
fn the_readme_states_the_schema_versions_this_build_speaks() {
    const README: &str = include_str!("../../../README.md");
    let stated: Vec<u32> = README
        .lines()
        .find(|l| l.contains("schema versions it speaks"))
        .and_then(|l| {
            let open = l.find('[')?;
            let close = l[open..].find(']')? + open;
            Some(
                l[open + 1..close]
                    .split(',')
                    .filter_map(|t| t.trim().trim_matches('`').parse().ok())
                    .collect(),
            )
        })
        .unwrap_or_default();
    assert!(
        !stated.is_empty(),
        "the README no longer states a schema version list where this test \
         looks — it read the sentence with `schema versions it speaks`. \
         Point this at the new wording rather than deleting the check."
    );
    assert_eq!(
        stated,
        smix_runner_wire::WIRE_SCHEMA_SUPPORTED,
        "README promises schemas {stated:?}, this build speaks {:?}",
        smix_runner_wire::WIRE_SCHEMA_SUPPORTED
    );
}

/// Every crate the README calls ABI-stable must be a crate that exists.
#[test]
fn the_readme_abi_stable_crates_all_exist() {
    const README: &str = include_str!("../../../README.md");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root");
    let line = README
        .lines()
        .find(|l| l.contains("ABI-stable within"))
        .expect("the README states an ABI-stable crate list");
    let named: Vec<&str> = line.split('`').filter(|t| t.starts_with("smix-")).collect();
    assert!(
        named.len() >= 5,
        "extracted only {named:?} from the ABI-stable sentence — the \
         extraction stopped matching and this check would pass by \
         knowing nothing"
    );
    let missing: Vec<&&str> = named
        .iter()
        .filter(|c| !root.join("crates").join(c).is_dir())
        .collect();
    assert!(
        missing.is_empty(),
        "the README promises ABI stability for crates that do not exist: \
         {missing:?}"
    );
}
