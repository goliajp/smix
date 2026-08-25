//! Which device does this port actually reach?
//!
//! `--device` is the knob a caller aims with, and nothing checked that
//! the port it was paired with goes anywhere near that device. An adb
//! forward can point a local port at a different emulator — or at a
//! phone on the desk — and every guard in §9 #1 still passes, because
//! they all read the `--device` string rather than asking who answers.
//!
//! It cost an hour to find out the hard way: a whole investigation was
//! run against `localhost:28080` believing it was `emulator-5554`, and
//! it was forwarded to `emulator-5556`. Two second-hand facts
//! disagreed for that hour — `dumpsys activity` naming one app and
//! `/windows` naming another — and they were both telling the truth
//! about two different machines.
//!
//! The runner cannot answer this. Both runners' `/health` are byte
//! identical across devices, so the authority has to be host-side and
//! live:
//!
//!   * Android — `adb forward --list`, which owns the mapping
//!   * iOS — the process listening on the port, whose path carries
//!     `CoreSimulator/Devices/<UDID>/`
//!
//! Deliberately NOT the runner registry. Its rows say who *started* a
//! runner, not who the port reaches now: at the time of writing it
//! carried two rows for port 28080 naming devices whose processes were
//! long gone, while the port itself reached a third.

/// What the host-side authority had to say about a port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Authority {
    /// It was asked, and these are the devices claiming the port. An
    /// empty list means it was asked and nobody claimed it — which is a
    /// finding, not an absence of one.
    Consulted(Vec<String>),
    /// There was nothing to ask. Physical iOS is reached through a
    /// tunnel this code has never been able to measure — there is no
    /// such device on the machine this was written on — so "no
    /// authority" must not be quietly turned into either answer.
    Unavailable { why: String },
}

/// What to do with a port whose ownership has been resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Safe to connect, and this is the device that will be driven.
    ///
    /// `unchecked` is `Some` when nothing could be consulted. The
    /// connection still happens — refusing would break a platform this
    /// cannot test — but it is never silent, because a pairing nobody
    /// verified reads exactly like one that was verified.
    Proceed {
        device: Option<String>,
        unchecked: Option<String>,
    },
    /// Do not connect. The sentence names both sides.
    Refuse { reason: String },
}

/// Decide whether `port` may be driven as `named`.
///
/// `owners` is what the host-side authority says holds this port. An
/// empty `owners` is **not** agreement — a predicate that passes when
/// its subject is missing has no subject (`gate/absence-needs-presence`).
pub fn decide(port: u16, named: Option<&str>, authority: &Authority) -> Verdict {
    let owners = match authority {
        Authority::Consulted(v) => v.as_slice(),
        Authority::Unavailable { why } => {
            return match named {
                Some(want) => Verdict::Proceed {
                    device: Some(want.to_string()),
                    unchecked: Some(format!(
                        "nothing could be asked who port {port} reaches ({why}), so \
                         driving it as {want} is unverified — if that port is \
                         forwarded elsewhere, this acts on the other device."
                    )),
                },
                // Naming no device means there is no aim to contradict,
                // and something IS serving the port. Refusing here would
                // buy no safety and would turn away every route that
                // reaches a runner by means this cannot interrogate — a
                // tunnel, a test double — so it proceeds and says so.
                None => Verdict::Proceed {
                    device: None,
                    unchecked: Some(format!(
                        "no device was named and nothing could be asked who port \
                         {port} reaches ({why}) — whatever serves that port is \
                         what this drives."
                    )),
                },
            };
        }
    };
    let mut seen: Vec<&str> = Vec::new();
    for o in owners {
        let o = o.trim();
        if !o.is_empty() && !seen.contains(&o) {
            seen.push(o);
        }
    }

    match (named, seen.as_slice()) {
        (Some(want), [only]) if *only == want => Verdict::Proceed {
            device: Some(want.to_string()),
            unchecked: None,
        },
        (Some(want), [only]) => Verdict::Refuse {
            reason: format!(
                "port {port} reaches {only}, and the command named {want}. \
                 Whoever holds the forward decides which device is driven, \
                 not the flag — so this would have acted on {only}."
            ),
        },
        (None, [only]) => Verdict::Proceed {
            device: Some(only.to_string()),
            unchecked: None,
        },
        // Nothing holds the port and nothing serves it. No action can
        // reach any device through it, so there is no misdirection to
        // catch — and refusing here buried the true sentence twice: a
        // runner that died mid-corpus came back as "no evidence it
        // reaches <udid>" on the retry, when what a reader needed was
        // "nothing is listening on that port".
        //
        // This guard is for a port that reaches SOMEONE ELSE. A port
        // that reaches nobody is the connection's story to tell.
        (Some(want), []) => Verdict::Proceed {
            device: Some(want.to_string()),
            unchecked: None,
        },
        // Nothing named and nothing claiming it: there is no aim to
        // protect, and the connection attempt is about to tell a better
        // story than this could. Refusing here replaced "no runner is
        // listening on that port" — the true and actionable sentence —
        // with a remark about claims, which is worse in exactly the
        // situation it fires.
        (None, []) => Verdict::Proceed {
            device: None,
            unchecked: None,
        },
        (want, many) => Verdict::Refuse {
            reason: format!(
                "port {port} is claimed by {} at once ({}), so which device it \
                 reaches is not decidable{}. An ambiguous port is not an \
                 agreeing port.",
                many.len(),
                many.join(", "),
                match want {
                    Some(w) => format!(", including for the named {w}"),
                    None => String::new(),
                }
            ),
        },
    }
}

/// Serials holding the local `port`, read off `adb forward --list`.
///
/// Its rows are `<serial> tcp:<local> tcp:<remote>`, and the local port
/// is the one being asked about — not the remote, which is the same
/// `28080` on every device and is exactly what makes these easy to
/// misread.
pub fn parse_adb_forwards(text: &str, port: u16) -> Vec<String> {
    let want = format!("tcp:{port}");
    text.lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            let serial = f.next()?;
            let local = f.next()?;
            (local == want).then(|| serial.to_string())
        })
        .collect()
}

/// The device a runner process is bound to, read off its command line.
///
/// Two shapes, because a simulator runner is launched into the
/// simulator's own container while a device runner is aimed with
/// `-destination id=`:
///
///   * `…/CoreSimulator/Devices/<UDID>/data/…`
///   * `… -destination id=<UDID> …`
pub fn udid_from_command(cmd: &str) -> Option<String> {
    if let Some(rest) = cmd.split("/CoreSimulator/Devices/").nth(1) {
        let udid = rest.split('/').next().unwrap_or_default();
        if !udid.is_empty() {
            return Some(udid.to_string());
        }
    }
    if let Some(rest) = cmd.split("id=").nth(1) {
        let udid: String = rest
            .chars()
            .take_while(|c| !c.is_whitespace() && *c != ',')
            .collect();
        if !udid.is_empty() {
            return Some(udid);
        }
    }
    None
}

/// Ask adb which device holds `port`.
pub fn ask_android(port: u16) -> Authority {
    match std::process::Command::new("adb")
        .args(["forward", "--list"])
        .output()
    {
        Ok(out) if out.status.success() => {
            let held = parse_adb_forwards(&String::from_utf8_lossy(&out.stdout), port);
            if !held.is_empty() {
                return Authority::Consulted(held);
            }
            // adb naming nobody is not the same as nobody being there.
            // The Android behaviour gate reaches its runner through a
            // wire-recording proxy — `runner up` forwards 28080, the
            // proxy listens on 28090 and relays — and a host process is
            // invisible to `adb forward --list`. Refusing there broke a
            // setup doing nothing wrong, seventeen minutes into a dry
            // run.
            //
            // Safety is unchanged: adb naming a DIFFERENT device still
            // refuses, because that returns above. Only when adb has no
            // answer at all does the question move to whoever is
            // listening, and a relay that cannot say which device it
            // reaches is treated the way a tunnel is on Apple — driven,
            // and said out loud.
            ask_ios(port)
        }
        Ok(out) => Authority::Unavailable {
            why: format!(
                "`adb forward --list` exited {}",
                out.status.code().unwrap_or(-1)
            ),
        },
        Err(e) => Authority::Unavailable {
            why: format!("`adb` could not be run: {e}"),
        },
    }
}

/// Ask the machine which simulator or device the process on `port` is
/// bound to.
///
/// A listener that names no device is `Unavailable` rather than an
/// empty `Consulted`: something IS serving the port, and this cannot
/// say what — which is a different fact from nothing serving it, and
/// only one of the two is safe to refuse on.
pub fn ask_ios(port: u16) -> Authority {
    let pids = match std::process::Command::new("lsof")
        .args(["-ti", &format!("tcp:{port}")])
        .output()
    {
        Ok(out) => String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| l.trim().parse::<u32>().ok())
            .collect::<Vec<_>>(),
        Err(e) => {
            return Authority::Unavailable {
                why: format!("`lsof` could not be run: {e}"),
            };
        }
    };
    if pids.is_empty() {
        return Authority::Consulted(Vec::new());
    }
    let mut found = Vec::new();
    for pid in &pids {
        if let Ok(out) = std::process::Command::new("ps")
            .args(["-o", "command=", "-p", &pid.to_string()])
            .output()
            && let Some(u) = udid_from_command(&String::from_utf8_lossy(&out.stdout))
        {
            found.push(u);
        }
    }
    if found.is_empty() {
        return Authority::Unavailable {
            why: format!(
                "{} process(es) serve the port and none of them names a device",
                pids.len()
            ),
        };
    }
    Authority::Consulted(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owners(v: &[&str]) -> Authority {
        Authority::Consulted(v.iter().map(|s| s.to_string()).collect())
    }

    const NONE: Authority = Authority::Consulted(Vec::new());

    #[test]
    fn a_relay_on_the_port_is_asked_about_rather_than_refused() {
        // What `ask_android` does when adb names nobody, expressed over
        // the decision rather than the shell: a port served by something
        // that cannot say which device it reaches is driven with a
        // warning, not refused. The gate's wire-recording proxy is
        // exactly that, and refusing it stopped a dry run.
        let v = decide(
            28090,
            Some("emulator-5554"),
            &Authority::Unavailable {
                why: "1 process(es) serve the port and none of them names a device".into(),
            },
        );
        let Verdict::Proceed { unchecked, .. } = v else {
            panic!("a relay that cannot name its device must not be refused");
        };
        assert!(unchecked.is_some(), "and it must not be silent about it");
    }

    #[test]
    fn adb_forwards_are_read_by_local_port_not_remote() {
        // Verbatim from `adb forward --list` on the machine where this
        // was found. Every row's remote is 28080, so a parser reading
        // the wrong column agrees with everything.
        let text = "emulator-5556 tcp:28080 tcp:28080\n\
                    emulator-5556 tcp:22089 tcp:28080\n\
                    emulator-5554 tcp:28090 tcp:28080\n";
        assert_eq!(parse_adb_forwards(text, 28090), vec!["emulator-5554"]);
        assert_eq!(parse_adb_forwards(text, 28080), vec!["emulator-5556"]);
        assert_eq!(parse_adb_forwards(text, 22089), vec!["emulator-5556"]);
        assert!(parse_adb_forwards(text, 22087).is_empty());
    }

    #[test]
    fn adb_forwards_survive_the_header_and_blank_lines() {
        assert!(parse_adb_forwards("List of devices attached\n\n", 28090).is_empty());
    }

    #[test]
    fn simulator_runner_names_its_device_in_its_path() {
        // Verbatim, truncated: the pid listening on 22087.
        let cmd = "/Users/x/Library/Developer/CoreSimulator/Devices/\
                   5D087114-ECB3-443C-8DDB-40EEF9CFB90C/data/Containers/Bundle/\
                   Application/28EBF37A/SmixRunnerUITests-Runner.app/SmixRunner";
        assert_eq!(
            udid_from_command(cmd).as_deref(),
            Some("5D087114-ECB3-443C-8DDB-40EEF9CFB90C")
        );
    }

    #[test]
    fn device_runner_names_its_device_in_its_destination() {
        let cmd = "xcodebuild test-without-building -destination id=00008120-000A1D2E3F -quiet";
        assert_eq!(
            udid_from_command(cmd).as_deref(),
            Some("00008120-000A1D2E3F")
        );
    }

    #[test]
    fn a_command_naming_no_device_yields_none() {
        assert_eq!(udid_from_command("/usr/bin/nc -l 22087"), None);
    }

    #[test]
    fn agreeing_port_proceeds() {
        assert_eq!(
            decide(28090, Some("emulator-5554"), &owners(&["emulator-5554"])),
            Verdict::Proceed {
                device: Some("emulator-5554".into()),
                unchecked: None
            }
        );
    }

    #[test]
    fn disagreeing_port_refuses_and_names_both_sides() {
        let v = decide(28080, Some("emulator-5554"), &owners(&["emulator-5556"]));
        let Verdict::Refuse { reason } = v else {
            panic!("a port reaching another device must not proceed");
        };
        // Both halves, or the reader cannot tell which way round it is.
        assert!(reason.contains("emulator-5554"), "reason: {reason}");
        assert!(reason.contains("emulator-5556"), "reason: {reason}");
        assert!(reason.contains("28080"), "reason: {reason}");
    }

    #[test]
    fn a_port_that_reaches_nobody_is_the_connections_story() {
        // The first cut refused here, and it cost two true sentences:
        // an iOS runner that died mid-corpus produced "no evidence it
        // reaches <udid>" on the retry, where "nothing is listening on
        // that port" was both true and actionable.
        //
        // Nothing is given up. This guard exists to catch a port that
        // reaches SOMEONE ELSE; a port that reaches nobody carries no
        // action to any device, so there is nothing to protect.
        assert_eq!(
            decide(28080, Some("emulator-5554"), &NONE),
            Verdict::Proceed {
                device: Some("emulator-5554".into()),
                unchecked: None
            }
        );
    }

    #[test]
    fn two_claimants_refuse() {
        let v = decide(
            28080,
            Some("emulator-5554"),
            &owners(&["emulator-5554", "emulator-5556"]),
        );
        assert!(
            matches!(v, Verdict::Refuse { .. }),
            "an ambiguous port is not an agreeing port, even when one claimant is ours"
        );
    }

    #[test]
    fn unnamed_adopts_the_single_owner_and_says_so() {
        assert_eq!(
            decide(28080, None, &owners(&["emulator-5556"])),
            Verdict::Proceed {
                device: Some("emulator-5556".into()),
                unchecked: None
            },
            "with no device named, the port's owner is the answer — stated, not assumed"
        );
    }

    #[test]
    fn unaskable_authority_proceeds_but_never_silently() {
        let v = decide(
            22087,
            Some("00008120-001"),
            &Authority::Unavailable {
                why: "no listener on the port".into(),
            },
        );
        let Verdict::Proceed { device, unchecked } = v else {
            panic!("refusing here would break a route this cannot measure");
        };
        assert_eq!(device.as_deref(), Some("00008120-001"));
        let said = unchecked.expect("an unverified pairing must say it is unverified");
        assert!(said.contains("22087"), "said: {said}");
        assert!(said.contains("00008120-001"), "said: {said}");
    }

    #[test]
    fn unaskable_authority_with_no_device_named_proceeds_but_says_so() {
        // Naming nothing means there is no aim to contradict, and
        // something is serving the port. The first cut refused here and
        // turned away every runner reached by means this cannot
        // interrogate — a tunnel, or the in-process mock two adapter
        // tests drive. That bought no safety: the danger this guards
        // against is a named device being silently replaced, and there
        // is no name here to replace.
        let v = decide(
            22087,
            None,
            &Authority::Unavailable {
                why: "1 process(es) serve the port and none of them names a device".into(),
            },
        );
        let Verdict::Proceed { device, unchecked } = v else {
            panic!("refusing here turns away routes that cannot be interrogated");
        };
        assert_eq!(device, None);
        assert!(
            unchecked.is_some_and(|s| s.contains("22087")),
            "an unverified pairing must still say it is unverified"
        );
    }

    #[test]
    fn unnamed_with_no_owner_leaves_the_story_to_the_connection() {
        // The first cut refused here, and it cost the SDK's own test of
        // the lazy path its message: with nothing named and nothing on
        // the port, the accurate sentence is "no runner is listening",
        // and this guard was overwriting it with a remark about claims.
        // The safety this guard exists for — a named device silently
        // replaced — has no subject here.
        assert_eq!(
            decide(28080, None, &NONE),
            Verdict::Proceed {
                device: None,
                unchecked: None
            }
        );
    }
}
