//! `smix init` — the bootstrap.
//!
//! Every smix command takes an explicit device, and an alias in `.smix` is
//! where that comes from. Getting the first one there meant `sim list`,
//! reading a UDID off the screen, and `sim register` — three steps and a
//! copy, before anything had been driven.
//!
//! What is decided here is separated from what is done: `plan_init` is a
//! pure function over the device list and the aliases already registered,
//! so the refusals — the part that matters, because each one is a place
//! someone could have been handed the wrong device — are testable without
//! a simulator.

/// A simulator that could be registered.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// Its UDID.
    pub udid: String,
    /// Its device name, for showing in a list of choices.
    pub name: String,
}

/// What init would do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitPlan {
    /// Alias to register under.
    pub alias: String,
    /// Device the alias will point at.
    pub udid: String,
}

/// Why init will not do it, and what to do instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitRefusal {
    /// What is in the way.
    pub reason: String,
    /// The command or flag that resolves it.
    pub remedy: String,
}

/// Decide what to register, or refuse with a way forward.
///
/// Never picks between several devices. An alias is what every later
/// command resolves through, so a guess here is a guess that silently
/// governs every flow written afterwards — the same reason the release
/// gate stopped taking "the first booted sim".
pub fn plan_init(
    devices: &[Candidate],
    alias: &str,
    requested_udid: Option<&str>,
    existing_aliases: &[String],
) -> Result<InitPlan, InitRefusal> {
    if devices.is_empty() {
        return Err(InitRefusal {
            reason: "no available simulator to register".into(),
            remedy: "create one first: xcrun simctl create smix-dev 'iPhone 17 Pro' <runtime-id>"
                .into(),
        });
    }

    if existing_aliases.iter().any(|a| a == alias) {
        return Err(InitRefusal {
            reason: format!("the alias `{alias}` is already registered"),
            remedy: format!(
                "register under a different name with --alias, or drive the existing one: \
                 smix capsule up {alias} --bundle <your.bundle.id>"
            ),
        });
    }

    let udid = match requested_udid {
        Some(want) => {
            if !devices.iter().any(|d| d.udid == want) {
                return Err(InitRefusal {
                    reason: format!("no available simulator with UDID {want}"),
                    remedy: "list what is available: smix sim list".into(),
                });
            }
            want.to_string()
        }
        None => {
            let [only] = devices else {
                let listed = devices
                    .iter()
                    .map(|d| format!("  {}  {}", d.udid, d.name))
                    .collect::<Vec<_>>()
                    .join("\n");
                return Err(InitRefusal {
                    reason: format!("{} simulators are available", devices.len()),
                    remedy: format!("name the one to register with --device <UDID>:\n{listed}"),
                });
            };
            only.udid.clone()
        }
    };

    Ok(InitPlan {
        alias: alias.to_string(),
        udid,
    })
}

/// The alias `smix init` gives a project when the caller does not name
/// one: the project directory's own name, lowercased, with anything
/// outside `[a-z0-9_-]` folded to `-` and runs collapsed. Two different
/// projects no longer both silently register as "dev". Falls back to
/// "dev" only when there is no usable directory name (the filesystem
/// root).
pub fn default_project_alias(cwd: &std::path::Path) -> String {
    let base = cwd
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mut out = String::new();
    let mut last_dash = false;
    for ch in base.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "dev".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_project_alias_from_dir_name() {
        use std::path::Path;
        assert_eq!(
            default_project_alias(Path::new("/Users/x/my-app")),
            "my-app"
        );
        assert_eq!(
            default_project_alias(Path::new("/Users/x/My App")),
            "my-app"
        );
        assert_eq!(
            default_project_alias(Path::new("/Users/x/smix.fixture")),
            "smix-fixture"
        );
        assert_eq!(default_project_alias(Path::new("/")), "dev");
    }

    fn devices() -> Vec<Candidate> {
        vec![
            Candidate {
                udid: "AAAA-1".into(),
                name: "iPhone 17 Pro".into(),
            },
            Candidate {
                udid: "BBBB-2".into(),
                name: "iPhone Air".into(),
            },
        ]
    }

    #[test]
    fn a_single_available_device_needs_no_choosing() {
        let plan = plan_init(&devices()[..1], "dev", None, &[]).expect("should plan");
        assert_eq!(plan.alias, "dev");
        assert_eq!(plan.udid, "AAAA-1");
    }

    #[test]
    fn several_devices_are_listed_rather_than_guessed() {
        let refusal = plan_init(&devices(), "dev", None, &[]).expect_err("ambiguous");
        assert!(refusal.reason.contains("2"), "{}", refusal.reason);
        assert!(refusal.remedy.contains("--device"), "{}", refusal.remedy);
        assert!(
            refusal.remedy.contains("AAAA-1"),
            "candidates belong in the remedy: {}",
            refusal.remedy
        );
    }

    #[test]
    fn an_existing_alias_is_never_silently_repointed() {
        let refusal =
            plan_init(&devices()[..1], "dev", None, &["dev".to_string()]).expect_err("alias taken");
        assert!(refusal.reason.contains("dev"));
        assert!(refusal.remedy.contains("--alias"));
    }

    #[test]
    fn a_named_device_that_is_not_there_is_named_back() {
        let refusal = plan_init(&devices(), "dev", Some("NOPE-9"), &[]).expect_err("unknown udid");
        assert!(refusal.reason.contains("NOPE-9"), "{}", refusal.reason);
    }

    #[test]
    fn no_devices_at_all_points_at_creating_one() {
        let refusal = plan_init(&[], "dev", None, &[]).expect_err("nothing to register");
        assert!(
            refusal.remedy.contains("simctl create"),
            "{}",
            refusal.remedy
        );
    }
}
