//! Which platform a command drives, read from the device it names.

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RunPlatform {
    Ios,
    Android,
}

/// The platform a run targets, read from the device it names rather
/// than from an argument that defaults to one.
///
/// `explicit` is `--platform` when the caller passed it — the override
/// for a device the registry cannot classify. Otherwise the device's
/// kind decides. When neither is available the answer is an error, not
/// a guess: guessing iOS is what sent an Android flow's launchApp to
/// simctl.
pub(crate) fn resolve_run_platform(
    explicit: Option<RunPlatform>,
    device_kind: Option<smix_simctl::registry::DeviceKind>,
) -> Result<RunPlatform, String> {
    use smix_simctl::registry::DeviceKind;
    if let Some(p) = explicit {
        return Ok(p);
    }
    match device_kind {
        Some(DeviceKind::Emulator) | Some(DeviceKind::PhysicalAndroid) => Ok(RunPlatform::Android),
        Some(DeviceKind::Simulator) | Some(DeviceKind::PhysicalIos) => Ok(RunPlatform::Ios),
        None => Err(
            "cannot tell which platform this device is — it is not in the registry, \
             so its kind is unknown. Register it (`smix sim register <alias> --udid \
             <id> --kind emulator|simulator`) so the platform is read from the \
             device, or pass --platform to say it once."
                .to_string(),
        ),
    }
}

impl RunPlatform {
    pub(crate) fn to_flow(self) -> smix_adapter_maestro::FlowPlatform {
        match self {
            Self::Ios => smix_adapter_maestro::FlowPlatform::Ios,
            Self::Android => smix_adapter_maestro::FlowPlatform::Android,
        }
    }
}

impl RunPlatform {
    pub(crate) fn to_driver(self) -> smix_driver::Platform {
        match self {
            Self::Ios => smix_driver::Platform::Ios,
            Self::Android => smix_driver::Platform::Android,
        }
    }
}

/// What the caller learned about a `--device` flag before dialing a
/// driver for a CLI verb. `Registered` carries the kind the registry
/// classified it as; `Unregistered` means `--device X` named something
/// the registry does not know. Absent `--device` is `None` — then the
/// dialed port's registered holder decides, the same way the consumer's
/// `--port 22088` usage does (5560 was registered with `--runner-port
/// 22088`).
pub(crate) enum DeviceDial {
    Registered(smix_simctl::registry::DeviceKind),
    Unregistered,
}

/// Dial the platform a CLI verb should drive, so `fill` / `find` reach
/// the same driver the flow path already uses instead of always the
/// simctl one. This is C1's rule — the platform is a property of the
/// device — brought to the act verbs' entrance: one rule, both entrances.
///
/// `--device` given and registered: its kind decides (reusing C1's
/// mapping). `--device` given but unknown to the registry: an error
/// naming the fix, never a silent iOS guess. No `--device`: the dialed
/// port's registered holders decide — one platform among them wins, none
/// keeps the bare-port iOS default (a wrong guess there ends in a loud
/// 501 from the Android runner, not a silent mis-drive), and two
/// different platforms on one port is an error rather than a coin toss.
pub(crate) fn dial_platform(
    device: Option<DeviceDial>,
    port_holder_kinds: &[smix_simctl::registry::DeviceKind],
) -> Result<RunPlatform, String> {
    match device {
        Some(DeviceDial::Registered(k)) => resolve_run_platform(None, Some(k)),
        Some(DeviceDial::Unregistered) => Err(
            "cannot tell which platform this --device is — it is not in the              registry, so its kind is unknown. Register it (`smix sim register              <alias> --udid <id> --kind emulator|simulator`) so the platform is              read from the device."
                .to_string(),
        ),
        None => {
            let mut platforms: Vec<RunPlatform> = Vec::new();
            for k in port_holder_kinds {
                let p = resolve_run_platform(None, Some(*k))
                    .expect("a known device kind always classifies");
                if !platforms.contains(&p) {
                    platforms.push(p);
                }
            }
            match platforms.as_slice() {
                [] => Ok(RunPlatform::Ios),
                [one] => Ok(*one),
                _ => Err(
                    "this runner port has devices of more than one platform                      registered on it (both an iOS and an Android device name it),                      so which one a verb without --device should drive is a coin                      toss. Pass --device to say which."
                        .to_string(),
                ),
            }
        }
    }
}
