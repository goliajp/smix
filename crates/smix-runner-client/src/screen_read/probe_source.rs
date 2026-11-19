//! Which smix-probe releases carry the probe this smix reads.
//!
//! The probe is built into the app and stays the version the app was built
//! with. Most releases do not touch it, so comparing version numbers alone
//! tells an app it is behind when its probe is the same code as today's.

/// The first release whose probe source is the one in this tree. A probe
/// from this release or later answers exactly as this smix expects.
pub(crate) const PROBE_SOURCE_SINCE: &str = "22.1.0";

/// sha256 of the probe source that [`PROBE_SOURCE_SINCE`] names, as
/// `scripts/dev/probe-source-since-is-current.py` computes it. That script
/// fails when the source and this digest part, which is when the release
/// above has to move to the version the change ships in.
#[expect(dead_code, reason = "read by the gate script, not by smix")]
pub(crate) const PROBE_SOURCE_DIGEST: &str =
    "b31d416b3f5e2e9832fa1b5a4a4059b1665d14e499fa75885643a2405120fb49";

/// What to say when the app's probe predates the probe source this smix
/// reads, or `None`.
#[must_use]
pub(crate) fn probe_version_note(probe: Option<&str>, ours: &str) -> Option<String> {
    let theirs = match probe {
        Some(v) if at_least(v, PROBE_SOURCE_SINCE) => return None,
        Some(v) => format!("is {v}"),
        None => "is from a release before probes said which they are".to_string(),
    };
    Some(format!(
        "smix: the app's smix-probe {theirs}, and the probe has changed since (the current one \
         dates from {PROBE_SOURCE_SINCE}) — those changes are not in effect until the app depends \
         on jp.golia.smix:smix-probe:{PROBE_SOURCE_SINCE} or later (this smix is {ours}) and is \
         rebuilt"
    ))
}

/// `a >= b` for `major.minor.patch`; anything else is not a version smix
/// published, so it is never at least one.
fn at_least(a: &str, b: &str) -> bool {
    fn parts(v: &str) -> Option<[u64; 3]> {
        let mut it = v.split('.').map(|p| p.parse::<u64>().ok());
        let out = [it.next()??, it.next()??, it.next()??];
        it.next().is_none().then_some(out)
    }
    matches!((parts(a), parts(b)), (Some(a), Some(b)) if a >= b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_probe_with_todays_source_says_nothing_whatever_its_number() {
        assert_eq!(probe_version_note(Some(PROBE_SOURCE_SINCE), "26.6.0"), None);
        assert_eq!(probe_version_note(Some("25.1.0"), "26.6.0"), None);
        assert_eq!(probe_version_note(Some("26.6.0"), "26.6.0"), None);
    }

    #[test]
    fn an_older_probe_is_named_with_what_to_depend_on() {
        let note = probe_version_note(Some("22.0.0"), "26.6.0").expect("a note");
        assert!(note.contains("is 22.0.0"), "{note}");
        assert!(
            note.contains(&format!("smix-probe:{PROBE_SOURCE_SINCE} or later")),
            "{note}"
        );
        let old = probe_version_note(None, "26.6.0").expect("a note");
        assert!(old.contains("before probes said which"), "{old}");
    }

    #[test]
    fn versions_compare_by_number_not_by_text() {
        assert!(at_least("22.10.0", "22.9.0"));
        assert!(!at_least("9.0.0", "22.1.0"));
        assert!(!at_least("22.1", "22.1.0"));
        assert!(!at_least("22.1.0-rc1", "22.1.0"));
    }
}
