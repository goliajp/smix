//! Which developer team signs the runner for a physical device.
//!
//! Building for a phone needs a team id. Xcode's own automatic signing
//! will pick the identity and the profile once it has one — the research
//! for this confirmed it: a single `DEVELOPMENT_TEAM=` plus
//! `-allowProvisioningUpdates` was enough to produce a signed device
//! build with nothing else configured.
//!
//! So the only question smix has to answer is *which team*, and IR-2
//! decides how: by finding it, not by asking somebody to edit an
//! xcconfig or export a variable. "Go change something on your machine
//! first" is not an answer a tool gets to give.
//!
//! Looking is separated from judging, the shape this codebase uses
//! everywhere: the caller runs `security` and reads the profiles, and
//! everything here is a pure function over what it found.

/// What was found on this machine.
#[derive(Debug, Clone, Default)]
pub struct SigningFacts {
    /// Team ids that have a usable development signing identity.
    ///
    /// Deduplicated — one team commonly has several certificates, and a
    /// person with two certs for one team does not have an ambiguity.
    pub teams: Vec<String>,
    /// Team ids whose provisioning profiles list the target device.
    ///
    /// Empty is not fatal: `-allowProvisioningUpdates` lets Xcode add a
    /// device to a profile it manages. It is worth reporting, though,
    /// because when Xcode cannot do that the failure lands far away.
    pub teams_with_device: Vec<String>,
}

/// Why a team could not be settled on.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SigningError {
    /// No development identity at all.
    #[error(
        "no Apple Development signing identity on this machine, so the runner cannot be \
         built for a physical device.\n\
         Add your Apple ID in Xcode > Settings > Accounts, then try again."
    )]
    NoIdentity,
    /// More than one, and no way to know which was meant.
    #[error(
        "this machine has development identities for {} teams: {}\n\
         Which one signs the runner is not something to guess — pass it:\n  \
         smix runner up <device> --team <TEAM_ID>",
        teams.len(),
        teams.join(", ")
    )]
    Ambiguous {
        /// The candidates, in the order found.
        teams: Vec<String>,
    },
}

/// Settle on the team that will sign the runner.
///
/// # Errors
///
/// [`SigningError::NoIdentity`] when there is nothing to sign with, and
/// [`SigningError::Ambiguous`] when there is more than one candidate.
///
/// Ambiguity is an error rather than a pick. Choosing for someone here
/// would sign a build with an identity they did not intend, and the
/// result is a runner that installs on a device belonging to the wrong
/// team's provisioning — a failure that surfaces minutes later as an
/// install error naming neither the team nor this decision. It is the
/// same rule `smix init` follows when several devices could be meant.
pub fn resolve_team(explicit: Option<&str>, facts: &SigningFacts) -> Result<String, SigningError> {
    // Said outright wins, and is not second-guessed against what was
    // found: a team that is configured but whose cert is not on this
    // machine yet is a state Xcode can resolve, and refusing it here
    // would be smix overruling the person who knows.
    if let Some(t) = explicit.map(str::trim).filter(|t| !t.is_empty()) {
        return Ok(t.to_string());
    }
    match facts.teams.as_slice() {
        [] => Err(SigningError::NoIdentity),
        [only] => Ok(only.clone()),
        // More than one identity, but only one of them can actually reach
        // this device — that is not ambiguity, it is an answer.
        many => match facts.teams_with_device.as_slice() {
            [only] if many.contains(only) => Ok(only.clone()),
            _ => Err(SigningError::Ambiguous {
                teams: facts.teams.clone(),
            }),
        },
    }
}

/// Gather signing facts from this machine.
///
/// Runs `security find-identity` and reads the provisioning profiles
/// Xcode keeps. Never judges — that is [`resolve_team`]'s job.
#[must_use]
pub fn collect_facts(device_udid: &str) -> SigningFacts {
    let mut teams = Vec::new();
    if let Ok(out) = std::process::Command::new("security")
        .args(["find-identity", "-v", "-p", "codesigning"])
        .output()
    {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            if !line.contains("Apple Development") {
                continue;
            }
            let Some(cn) = common_name(line) else {
                continue;
            };
            if let Some(t) = team_of_certificate(&cn)
                && !teams.iter().any(|k| k == &t)
            {
                teams.push(t);
            }
        }
    }

    let teams_with_device = profile_teams_for_device(device_udid)
        .into_iter()
        .filter(|t| teams.iter().any(|k| k == t))
        .collect();

    SigningFacts {
        teams,
        teams_with_device,
    }
}

/// Pull the quoted common name out of a `security find-identity` line.
///
/// Lines look like:
/// `  2) <SHA1> "Apple Development: HAO LI (W6GKU3U95X)"`
fn common_name(line: &str) -> Option<String> {
    let start = line.find('"')? + 1;
    let rest = &line[start..];
    let end = rest.rfind('"')?;
    Some(rest[..end].to_string())
}

/// The team id a signing certificate belongs to.
///
/// **Not** the value in the common name's parentheses. That was the bug
/// here until 2026-08-06, and only a real device could surface it: the
/// runner refused to guess between two "teams" that were not teams at
/// all, and told the user to pass one of them to `--team`, where it would
/// have failed again further downstream and further from the cause.
///
/// Measured on this machine:
///
/// ```text
/// CN=Apple Development: HAO LI (W6GKU3U95X), OU=KF79DRC524, O=GOLIA K.K.
/// CN=Apple Development: HAO LI (79R357HB86), OU=QC48NH8Z94, O=Focusai Inc
/// ```
///
/// The parenthesised value identifies the *developer*; `OU` is the team,
/// and it is the one that matches a provisioning profile's
/// `TeamIdentifier`. `security find-identity` only prints the common
/// name, so the certificate itself has to be read.
fn team_of_certificate(common_name: &str) -> Option<String> {
    let pem = std::process::Command::new("security")
        .args(["find-certificate", "-c", common_name, "-p"])
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let subject = std::process::Command::new("openssl")
        .args(["x509", "-noout", "-subject"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .ok()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.take()?.write_all(&pem.stdout).ok()?;
            child.wait_with_output().ok()
        })
        .filter(|o| o.status.success())?;
    subject_ou(&String::from_utf8_lossy(&subject.stdout))
}

/// Read `OU=` out of an `openssl x509 -subject` line.
fn subject_ou(subject: &str) -> Option<String> {
    let at = subject.find("OU=")? + "OU=".len();
    let rest = &subject[at..];
    let end = rest.find(',').unwrap_or(rest.len());
    let ou = rest[..end].trim();
    (!ou.is_empty()).then(|| ou.to_string())
}

/// Team ids whose profiles list this device.
fn profile_teams_for_device(udid: &str) -> Vec<String> {
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    let dir =
        std::path::Path::new(&home).join("Library/Developer/Xcode/UserData/Provisioning Profiles");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut teams = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "mobileprovision") {
            continue;
        }
        // A profile is CMS-signed; the plist inside is plain text, so the
        // device list and the team id can be read without verifying the
        // signature. Verification is Xcode's job, not ours — all this
        // needs to know is which teams *might* reach the device.
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        if !text.contains(udid) {
            continue;
        }
        if let Some(team) = extract_first_team(&text)
            && !teams.contains(&team)
        {
            teams.push(team);
        }
    }
    teams
}

/// Pull the first `TeamIdentifier` entry out of a profile's plist text.
fn extract_first_team(text: &str) -> Option<String> {
    let at = text.find("TeamIdentifier")?;
    let rest = &text[at..];
    let open = rest.find("<string>")? + "<string>".len();
    let close = rest[open..].find("</string>")?;
    Some(rest[open..open + close].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(teams: &[&str], with_device: &[&str]) -> SigningFacts {
        SigningFacts {
            teams: teams.iter().map(|s| (*s).to_string()).collect(),
            teams_with_device: with_device.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    #[test]
    fn one_team_is_the_team() {
        assert_eq!(
            resolve_team(None, &facts(&["KF79DRC524"], &[])).unwrap(),
            "KF79DRC524"
        );
    }

    #[test]
    fn no_identity_says_what_to_do_and_does_not_mention_xcconfig() {
        // IR-2: "go edit a file on your machine first" is not an answer a
        // tool gets to give. Pointing at Xcode's account settings is
        // different — that is where the thing genuinely lives.
        let e = resolve_team(None, &facts(&[], &[])).unwrap_err();
        let msg = e.to_string();
        assert!(msg.contains("Xcode > Settings > Accounts"), "got: {msg}");
        assert!(!msg.contains("xcconfig"), "got: {msg}");
        assert!(
            !msg.to_lowercase().contains("environment variable"),
            "got: {msg}"
        );
    }

    #[test]
    fn two_teams_is_an_error_not_a_pick() {
        // Choosing here signs a build with an identity nobody intended,
        // and it surfaces minutes later as an install failure naming
        // neither the team nor this decision.
        let e = resolve_team(None, &facts(&["KF79DRC524", "QC48NH8Z94"], &[])).unwrap_err();
        let msg = e.to_string();
        assert!(
            msg.contains("KF79DRC524") && msg.contains("QC48NH8Z94"),
            "got: {msg}"
        );
        assert!(
            msg.contains("--team"),
            "the way to resolve it must be named: {msg}"
        );
    }

    #[test]
    fn two_teams_but_only_one_reaches_the_device_is_not_ambiguous() {
        // This is an answer, not a coin flip: the other team's profiles
        // do not list this phone, so it could not sign for it anyway.
        assert_eq!(
            resolve_team(None, &facts(&["KF79DRC524", "QC48NH8Z94"], &["QC48NH8Z94"])).unwrap(),
            "QC48NH8Z94"
        );
    }

    #[test]
    fn two_teams_that_both_reach_the_device_stay_ambiguous() {
        assert!(matches!(
            resolve_team(
                None,
                &facts(&["KF79DRC524", "QC48NH8Z94"], &["KF79DRC524", "QC48NH8Z94"])
            ),
            Err(SigningError::Ambiguous { .. })
        ));
    }

    #[test]
    fn an_explicit_team_is_used_without_second_guessing() {
        // A team whose certificate is not on this machine yet is a state
        // Xcode can resolve; refusing it would be smix overruling the
        // person who knows.
        assert_eq!(
            resolve_team(Some("EXPLICIT99"), &facts(&["KF79DRC524"], &[])).unwrap(),
            "EXPLICIT99"
        );
        assert_eq!(
            resolve_team(Some("EXPLICIT99"), &facts(&[], &[])).unwrap(),
            "EXPLICIT99"
        );
    }

    #[test]
    fn an_empty_team_flag_is_treated_as_absent() {
        // `--team ""` is a mistake, not an instruction.
        assert!(matches!(
            resolve_team(Some("  "), &facts(&[], &[])),
            Err(SigningError::NoIdentity)
        ));
    }

    #[test]
    fn the_team_comes_from_ou_not_from_the_common_name() {
        // The bug a real phone found. Both halves of this line were on
        // this machine on 2026-08-06: the parenthesised value identifies
        // the developer, `OU` identifies the team, and it is `OU` that
        // matches a provisioning profile's `TeamIdentifier`.
        let subject = "subject=UID=65BU7CWKDP, CN=Apple Development: HAO LI (W6GKU3U95X), \
             OU=KF79DRC524, O=GOLIA K.K., C=US";
        assert_eq!(subject_ou(subject).as_deref(), Some("KF79DRC524"));
        assert_ne!(
            subject_ou(subject).as_deref(),
            Some("W6GKU3U95X"),
            "the common name's parentheses are not the team"
        );
        assert_eq!(subject_ou("subject=CN=nothing useful, C=US"), None);
    }

    #[test]
    fn a_common_name_is_read_out_of_a_find_identity_line() {
        assert_eq!(
            common_name(r#"  2) A1B2 "Apple Development: HAO LI (W6GKU3U95X)""#).as_deref(),
            Some("Apple Development: HAO LI (W6GKU3U95X)")
        );
        assert_eq!(common_name("  0 identities found"), None);
    }

    #[test]
    fn a_team_identifier_is_read_out_of_profile_text() {
        let text = r"<key>TeamIdentifier</key><array><string>KF79DRC524</string></array>";
        assert_eq!(extract_first_team(text).as_deref(), Some("KF79DRC524"));
        assert_eq!(extract_first_team("no team here"), None);
    }
}
