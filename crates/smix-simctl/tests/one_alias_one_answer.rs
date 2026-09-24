//! One alias, one answer — and the book it came from.
//!
//! Device facts belong to the machine (§9 #9). A checkout may still carry
//! a pre-store `.smix/sims.json`, and it keeps exactly two powers: to stop
//! a decision when it disagrees with the machine, and to be named as
//! evidence. Measured on 2026-09-24, it had a third: when the two books
//! gave one alias to two devices, the merge kept whichever UDID sorted
//! first, so the checkout won half the time and the alias drove the device
//! it named. The note printed beside it said the alias was "not on this
//! machine", which was false.
//!
//! Every case is built in temporary directories handed to
//! [`SimRegistry::open_books`]; nothing here reads the environment or the
//! machine's real registry.

use smix_simctl::registry::{
    DeviceKind, RegisteredSim, RegistryError, Resolved, SimRegistry, Source,
};
use std::path::{Path, PathBuf};

const A: &str = "986DA42B-E0B0-4CCE-8E94-3510C85E8044";
const B: &str = "89980B43-EF26-446A-A897-848C1AD3A872";

fn sim(udid: &str) -> RegisteredSim {
    RegisteredSim {
        device_name: udid.into(),
        kind: DeviceKind::Simulator,
        destructive_opt_in: false,
        udid: udid.into(),
        runtime: String::new(),
        device_type: String::new(),
        avd_name: None,
        locale: None,
        runner_port: None,
    }
}

fn machine_with(dir: &Path, alias: &str, udid: &str) -> PathBuf {
    let m = dir.join("machine").join("devices");
    SimRegistry::register(&m, alias, sim(udid)).expect("register on the machine");
    m
}

/// A checkout carrying only the legacy file, as a consumer's does.
fn checkout_with(dir: &Path, alias: &str, udid: &str) -> PathBuf {
    let smix = dir.join("checkout").join(".smix");
    std::fs::create_dir_all(&smix).unwrap();
    std::fs::write(
        smix.join("sims.json"),
        format!(
            r#"{{"sims":{{"{alias}":{{"udid":"{udid}","deviceType":"","runtime":"","deviceName":"{alias}"}}}}}}"#
        ),
    )
    .unwrap();
    smix
}

fn tmp() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

#[test]
fn two_books_that_disagree_stop_the_answer_when_the_checkout_sorts_first() {
    let d = tmp();
    let m = machine_with(d.path(), "phone", A);
    let c = checkout_with(d.path(), "phone", B); // B < A
    let view = SimRegistry::open_books(Some(&m), std::slice::from_ref(&c));
    match view.resolve_ref("phone") {
        Err(RegistryError::Diverged {
            alias,
            machine,
            checkouts,
        }) => {
            assert_eq!(alias, "phone");
            assert_eq!(machine.1, A);
            assert_eq!(checkouts, vec![(c.join("sims.json"), B.to_string())]);
        }
        other => panic!("the two books name two devices and this answered {other:?}"),
    }
}

#[test]
fn two_books_that_disagree_stop_the_answer_when_the_machine_sorts_first() {
    let d = tmp();
    let m = machine_with(d.path(), "phone", B);
    let c = checkout_with(d.path(), "phone", A); // B < A: the machine's sorts first
    let view = SimRegistry::open_books(Some(&m), &[c]);
    assert!(
        matches!(
            view.resolve_ref("phone"),
            Err(RegistryError::Diverged { .. })
        ),
        "a disagreement is a disagreement whichever UDID sorts first; got {:?}",
        view.resolve_ref("phone")
    );
}

#[test]
fn an_alias_only_the_machine_holds_answers_from_the_machine() {
    let d = tmp();
    let m = machine_with(d.path(), "phone", A);
    let view = SimRegistry::open_books(Some(&m), &[]);
    let Resolved { id, alias, source } = view.resolve_ref("phone").unwrap();
    assert_eq!((id.as_str(), alias.as_str()), (A, "phone"));
    assert_eq!(source, Source::Machine(m));
}

#[test]
fn an_alias_only_a_checkout_holds_answers_from_it_and_says_so() {
    let d = tmp();
    let m = d.path().join("machine").join("devices");
    let c = checkout_with(d.path(), "phone", B);
    let view = SimRegistry::open_books(Some(&m), std::slice::from_ref(&c));
    let r = view.resolve_ref("phone").unwrap();
    assert_eq!(r.id, B);
    assert_eq!(r.source, Source::Checkout(c.join("sims.json")));
    assert!(view.unmigrated.contains_key("phone"));
}

#[test]
fn a_disagreement_is_not_reported_as_an_alias_missing_from_the_machine() {
    let d = tmp();
    let m = machine_with(d.path(), "phone", A);
    let c = checkout_with(d.path(), "phone", B);
    let view = SimRegistry::open_books(Some(&m), &[c]);
    assert!(
        !view.unmigrated.contains_key("phone"),
        "`phone` is on this machine — it names another device there — and \
         calling it unmigrated sends the reader to move a record that is not missing"
    );
}

#[test]
fn reading_a_checkout_writes_nothing_into_it() {
    let d = tmp();
    let c = checkout_with(d.path(), "phone", B);
    let before: Vec<_> = std::fs::read_dir(&c)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    let _ = SimRegistry::open_books(None, std::slice::from_ref(&c));
    let after: Vec<_> = std::fs::read_dir(&c)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(
        before, after,
        "reading a checkout's legacy book created files in it — device facts are \
         not written back to a checkout (§9 #9)"
    );
}
