//! Merging the per-checkout registries into one machine-level registry.
//!
//! Four checkouts on this machine each keep a `.smix/`, because
//! `registry_path()` walks up from the working directory. They describe
//! the same simulators — the same UDIDs, sometimes under different
//! aliases, and each with its own answer about destructive consent.
//!
//! Merging them is a one-way door: whatever this drops, the workspace
//! that was relying on it stops working, and the person will not know
//! which of their four trees to look in. So the rules here are chosen
//! to be safe rather than tidy.
//!
//! - **Two aliases for one UDID: keep both.** An alias is how somebody
//!   types a device's name; dropping one breaks whatever script used it,
//!   and a duplicate is only noise.
//! - **Conflicting destructive consent: take the stricter.** Consent is
//!   a per-device authorisation (§9 #1), and merging two books is not a
//!   moment to widen it. Someone who granted it in one tree can grant it
//!   again; someone who did not cannot un-wipe a phone.
//! - **A corrupt source is skipped and named, not fatal.** One unreadable
//!   file must not strand the other three.

use smix_simctl::registry::{DeviceKind, RegisteredSim, SimRegistry};

fn sim(udid: &str, kind: DeviceKind, opt_in: bool) -> RegisteredSim {
    RegisteredSim {
        device_name: udid.into(),
        kind,
        destructive_opt_in: opt_in,
        udid: udid.into(),
        runtime: String::new(),
        device_type: String::new(),
        locale: None,
        runner_port: None,
    }
}

/// The same device under two names, from two checkouts.
#[test]
fn two_aliases_for_one_device_both_survive() {
    let mut a = SimRegistry::default();
    a.insert("dev", sim("UDID-1", DeviceKind::Simulator, false));
    let mut b = SimRegistry::default();
    b.insert("sim-01", sim("UDID-1", DeviceKind::Simulator, false));

    let merged = SimRegistry::merge([a, b]);

    assert!(
        merged.lookup("dev").is_some() && merged.lookup("sim-01").is_some(),
        "an alias is how somebody types a device's name; dropping one breaks \
         whatever script used it, and keeping both costs a line"
    );
    assert_eq!(merged.lookup("dev").unwrap().udid, "UDID-1");
    assert_eq!(merged.lookup("sim-01").unwrap().udid, "UDID-1");
}

/// Consent does not widen when two books disagree.
#[test]
fn conflicting_destructive_consent_takes_the_stricter() {
    let mut permissive = SimRegistry::default();
    permissive.insert("phone", sim("UDID-P", DeviceKind::PhysicalIos, true));
    let mut strict = SimRegistry::default();
    strict.insert("phone", sim("UDID-P", DeviceKind::PhysicalIos, false));

    for pair in [[permissive.clone(), strict.clone()], [strict, permissive]] {
        let merged = SimRegistry::merge(pair);
        assert!(
            !merged.lookup("phone").unwrap().destructive_opt_in,
            "merging two books is not a moment to widen a per-device \
             authorisation. Granting it again is one command; un-wiping a \
             phone is not a command at all."
        );
    }
}

/// Same alias, different devices — both kept, one renamed rather than
/// silently overwritten.
#[test]
fn the_same_alias_for_different_devices_does_not_lose_one() {
    let mut a = SimRegistry::default();
    a.insert("dev", sim("UDID-A", DeviceKind::Simulator, false));
    let mut b = SimRegistry::default();
    b.insert("dev", sim("UDID-B", DeviceKind::Simulator, false));

    let merged = SimRegistry::merge([a, b]);

    let udids: Vec<String> = merged.all().map(|(_, s)| s.udid.clone()).collect();
    assert!(
        udids.contains(&"UDID-A".to_string()) && udids.contains(&"UDID-B".to_string()),
        "one of these would stop resolving, in whichever tree relied on it, \
         with no way to tell which. found: {udids:?}"
    );
}

/// An empty source contributes nothing and breaks nothing.
#[test]
fn an_empty_source_is_not_fatal() {
    let mut a = SimRegistry::default();
    a.insert("dev", sim("UDID-1", DeviceKind::Simulator, false));
    let merged = SimRegistry::merge([a, SimRegistry::default()]);
    assert!(merged.lookup("dev").is_some());
}

/// Merging is not order-dependent for the facts that matter.
///
/// Four checkouts have no natural order, and a merge whose result
/// depends on the order they were read is one that changes when
/// somebody renames a directory.
#[test]
fn the_result_does_not_depend_on_the_order_of_the_books() {
    let mut a = SimRegistry::default();
    a.insert("one", sim("UDID-1", DeviceKind::Simulator, false));
    let mut b = SimRegistry::default();
    b.insert("two", sim("UDID-2", DeviceKind::Emulator, false));

    let ab: Vec<String> = SimRegistry::merge([a.clone(), b.clone()])
        .all()
        .map(|(k, _)| k.to_string())
        .collect();
    let ba: Vec<String> = SimRegistry::merge([b, a])
        .all()
        .map(|(k, _)| k.to_string())
        .collect();

    assert_eq!(
        ab, ba,
        "a merge that depends on read order changes when a \
                        directory is renamed"
    );
}
