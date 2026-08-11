//! Every `sim` verb is classified, and the destructive ones are governed.
//!
//! `guard_sim_verb` runs once for every verb, before any of them; the
//! comment above it says why: "putting it inside the arms would mean
//! remembering it in each — and the arms that most needed it are exactly
//! the ones nobody remembered."
//!
//! `guard_destructive` — the gate §9 #1 names — is inside the arms, at
//! three of them. So on a physical device the transport gate speaks
//! first ("this command runs through simctl, and that is a phone") and
//! the governance gate never does. `v2.3-c15` and `v2.3-c10` both fail
//! on that, one root cause.
//!
//! The device is safe today, but by a routing fact rather than a rule.
//! The guard's own comment saw this coming: what stopped a phone was
//! simctl not recognising it downstream, "a property of the executor,
//! not of this guard, and it stopped holding the day a `devicectl` path
//! appeared". RFC 2.3's whole direction is making physical devices
//! reachable.
//!
//! Which verbs are destructive cannot be derived — `exec` runs an
//! arbitrary command and is at least as dangerous as `uninstall`. So
//! this does not guess. It requires every variant to be in exactly one
//! of two lists, the way the route scan does, because "not mentioned"
//! is how three routes went two major versions without reading the
//! header they needed.

const MAIN: &str = include_str!("../src/main.rs");

/// Verbs that can take something away from a device somebody carries.
///
/// Each must reach `guard_destructive`.
const DESTRUCTIVE: &[&str] = &[
    "Erase",
    "Uninstall",
    "KeychainReset",
    // Runs an arbitrary command on the device. Whatever the worst thing
    // that command can do is, this verb can do it — which makes it no
    // less destructive than `uninstall`, and it was not guarded.
    "Exec",
];

/// Verbs that cannot, and why. Listed rather than assumed: silence is
/// how a new verb would arrive ungoverned.
const NOT_DESTRUCTIVE: &[(&str, &str)] = &[
    ("List", "reads"),
    ("Resolve", "reads"),
    (
        "Register",
        "writes this workspace's registry, not the device",
    ),
    ("Boot", "powering on takes nothing away"),
    (
        "Shutdown",
        "disruptive, and the device comes back with everything on it",
    ),
    ("Screenshot", "reads"),
    ("Launch", "starts an app"),
    ("Terminate", "stops an app; its data stays"),
    (
        "Install",
        "adds; the uninstall that would remove it is guarded",
    ),
    ("Openurl", "hands a URL to the system"),
    (
        "Appearance",
        "light or dark, and reversible by the same verb",
    ),
    (
        "AllowDestructive",
        "grants the consent; gating it on consent is a circle",
    ),
    ("Locale", "reversible by the same verb"),
    (
        "Unregister",
        "takes away a name, not anything on the device; another alias for \
         the same device keeps working, and re-registering restores it",
    ),
    (
        "Migrate",
        "copies records between books and removes nothing from either",
    ),
];

/// Every `SimAction` variant, read out of the enum.
fn variants() -> Vec<String> {
    let start = MAIN.find("enum SimAction {").expect("SimAction enum");
    let body = &MAIN[start..];
    let end = body.find("\n}\n").expect("end of SimAction");
    body[..end]
        .lines()
        .filter_map(|l| {
            let t = l.trim_end();
            let name = t.strip_prefix("    ")?;
            if !name.chars().next()?.is_ascii_uppercase() {
                return None;
            }
            let name = name.split(&[' ', '{', '(', ','][..]).next()?;
            (!name.is_empty()).then(|| name.to_string())
        })
        .collect()
}

#[test]
fn every_verb_is_classified() {
    let vs = variants();
    assert!(
        vs.len() >= 12,
        "only {} variants parsed out of SimAction — the enum's shape changed and \
         this test is reading air.\n{vs:?}",
        vs.len()
    );
    for v in &vs {
        let d = DESTRUCTIVE.contains(&v.as_str());
        let n = NOT_DESTRUCTIVE.iter().any(|(name, _)| name == v);
        assert!(
            d != n,
            "`{v}` is in {} of the two lists. Say which it is: can it take \
             something away from a device somebody carries?",
            if d { "both" } else { "neither" }
        );
    }
}

/// The point of the whole file: a destructive verb reaches the
/// governance gate.
#[test]
fn every_destructive_verb_reaches_the_governance_gate() {
    // Comments stripped: this file's own prose names the verbs it is
    // about, and a scan reading it whole would pass on the strength of
    // the explanation. Three gates in this cycle were caught that way.
    let code: String = MAIN
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");

    // One entry point, not one call per arm. The arms are what went
    // wrong: three had the call, the rest did not, and `guard_sim_verb`
    // sits at the entry precisely because per-arm guarding is forgotten.
    let entry = code.contains("guard_destructive_action(&action")
        || code.contains("guard_destructive_entry(&action");
    assert!(
        entry,
        "no single entry point calls the governance gate for every verb. \
         Guarding inside the arms is what left it silent on a physical \
         device — `guard_sim_verb` is at the entry for exactly this reason."
    );

    for v in DESTRUCTIVE {
        assert!(
            code.contains(&format!("SimAction::{v}")),
            "`{v}` is listed destructive and does not appear in main.rs — \
             renamed, or the list is stale"
        );
    }
}
