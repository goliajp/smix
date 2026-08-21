//! What a contract file is, and what it refuses.
//!
//! A contract is one sentence about what the app owes, carrying an id.
//! The sentence is the part people already write — in the comment above
//! a flow, in a ticket, in a design doc — and the id is the part they
//! do not, which is why nothing can reconcile the sentence against what
//! any platform actually covers.
//!
//! Two refusals matter more than the parse itself:
//!
//! A contract without an id cannot be claimed by anything, so it is
//! not a contract; it is a sentence. Accepting it would put a
//! requirement in the file that no reconciliation can ever mention.
//!
//! A duplicate id is worse than either. One id pointing at two
//! requirements makes every later answer meaningless — a claim on that
//! id says nothing about which of the two it covers, and the crate's
//! whole output is claims joined to ids.

use smix_contract::{ContractError, parse_contracts};

#[test]
fn a_minimal_contract_is_an_id_and_a_sentence() {
    let text = "\
- id: CTR-0001
  statement: Pausing notifications from the camera card, and taking it back
";
    let contracts = parse_contracts(text, "contracts.yaml").expect("should parse");
    assert_eq!(contracts.len(), 1);
    assert_eq!(contracts[0].id, "CTR-0001");
    assert_eq!(
        contracts[0].statement,
        "Pausing notifications from the camera card, and taking it back"
    );
    assert_eq!(contracts[0].origin, "contracts.yaml");
}

#[test]
fn a_contract_without_an_id_is_refused_by_name() {
    let text = "- statement: Something the app owes\n";
    let err = parse_contracts(text, "contracts.yaml").expect_err("should refuse");
    let said = err.to_string();
    assert!(
        said.contains("id"),
        "the refusal must name the field that is missing, said: {said}"
    );
    assert!(
        said.contains("contracts.yaml"),
        "the refusal must name where it read that, said: {said}"
    );
    assert!(
        matches!(err, ContractError::MissingField { .. }),
        "a missing field is its own kind, not a generic parse error"
    );
}

#[test]
fn a_contract_without_a_statement_is_refused_by_name() {
    let text = "- id: CTR-0001\n";
    let err = parse_contracts(text, "contracts.yaml").expect_err("should refuse");
    assert!(err.to_string().contains("statement"), "said: {err}");
}

#[test]
fn the_same_id_twice_is_refused_and_both_places_are_named() {
    let text = "\
- id: CTR-0001
  statement: The first thing
- id: CTR-0002
  statement: Something else
- id: CTR-0001
  statement: A different thing wearing the same id
";
    let err = parse_contracts(text, "contracts.yaml").expect_err("should refuse");
    let said = err.to_string();
    assert!(said.contains("CTR-0001"), "said: {said}");
    // Both positions, because "it is a duplicate" without saying of what
    // leaves the reader to find the other one.
    assert!(
        said.contains('1') && said.contains('3'),
        "the refusal must name both entries (1 and 3), said: {said}"
    );
    assert!(matches!(err, ContractError::DuplicateId { .. }));
}

#[test]
fn an_empty_file_is_no_contracts_rather_than_an_error() {
    // An empty file is a legitimate state — a corpus that has not been
    // written yet. It is the RECONCILIATION that must refuse to call
    // an empty corpus agreement; parsing has nothing to object to.
    let contracts = parse_contracts("", "contracts.yaml").expect("should parse");
    assert!(contracts.is_empty());
}

#[test]
fn a_blank_statement_is_refused() {
    // A present-but-empty statement is the shape that passes a
    // "the field is there" check and says nothing. The id would then
    // reconcile perfectly while standing for no requirement at all.
    let text = "- id: CTR-0001\n  statement: \"\"\n";
    let err = parse_contracts(text, "contracts.yaml").expect_err("should refuse");
    assert!(err.to_string().contains("statement"), "said: {err}");
}
