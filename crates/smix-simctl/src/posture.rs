//! Folding a foldable simulator, with no window involved.
//!
//! Xcode 27's Device Hub folds an iPhone Duo by sending a hinge angle to the
//! simulator as a vendor-defined HID report: CoreDevice's
//! `remote.hid.vendordefined` service, an `IndigoVendorDefinedEvent` with
//! usage page 0xff61, usage 0x5b, and an IOKit-serialised dictionary whose
//! `value` is the angle in degrees (0 closed, 180 flat). smix sends the same
//! report, so a fold needs neither Device Hub nor the Accessibility
//! permission, and nothing appears on screen.
//!
//! The simulator creates the HID services that carry the report when the
//! first message arrives and tears them down when the sender disconnects,
//! so the connection stays open until the fold shows: the panel the posture
//! lights reports itself active (measured 1.4–2.7 s).

use crate::displays::{Screen, read_screens};
use crate::{DeviceControlError, SimctlClient};
use std::time::{Duration, Instant};

/// How long a fold may take to show before it is called failed. Sent once,
/// folds showed in 0.9–4.7 s, in two clusters, the slow one a first report
/// that came to nothing; sent again every second, 1.4–2.7 s over 12 folds.
/// A ceiling, not a wait.
const SHOWS_WITHIN: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(100);
/// How often the report is sent again while the device has not shown it.
/// A report can be taken and come to nothing — once in a run of dozens a
/// facing never arrived and the fifteen seconds ran out — and the value is
/// absolute, so sending it again is harmless.
const RESEND: Duration = Duration::from_secs(1);
/// How long the connection stays open when the lit panel cannot tell
/// whether the report took (half-open and open light the same panel). The
/// simulator drops the services that carry the report when the sender
/// disconnects: held 0.3 s, 0 of 4 folds took; 0.6 s, 4 of 4; 1.5 s, 14
/// of 14.
const HOLD_WHEN_UNOBSERVABLE: Duration = Duration::from_millis(1500);

/// How far a foldable is opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Posture {
    /// Folded shut: the outer panel is lit.
    Closed,
    /// Opened partway, like a laptop: the inner panel is lit.
    HalfOpen,
    /// Opened flat: the inner panel is lit.
    Open,
}

impl Posture {
    /// The hinge angle sent for this posture, in degrees.
    pub fn degrees(self) -> f64 {
        match self {
            Posture::Closed => 0.0,
            Posture::HalfOpen => 90.0,
            Posture::Open => 180.0,
        }
    }

    /// The name used on the command line and in flows.
    pub fn name(self) -> &'static str {
        match self {
            Posture::Closed => "closed",
            Posture::HalfOpen => "half-open",
            Posture::Open => "open",
        }
    }

    /// Whether the posture lights the main (outer) panel.
    fn lights_main(self) -> bool {
        match self {
            Posture::Closed => true,
            Posture::HalfOpen | Posture::Open => false,
        }
    }
}

impl std::str::FromStr for Posture {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "closed" => Ok(Posture::Closed),
            "half-open" | "halfOpen" => Ok(Posture::HalfOpen),
            "open" => Ok(Posture::Open),
            other => Err(format!(
                "`{other}` is not a posture; use closed, half-open or open"
            )),
        }
    }
}

/// What the screens say about a fold that was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Readback {
    /// The panel the posture lights is the lit one.
    Shown,
    /// Not yet, or not at all: the lit panel is still the other one.
    NotYet,
    /// The device has one screen; it cannot fold.
    NotFoldable,
}

/// Judge a fold against what devicectl reads.
pub fn read_back(posture: Posture, screens: &[Screen]) -> Readback {
    if screens.len() <= 1 {
        return Readback::NotFoldable;
    }
    let lit_main = screens.iter().any(|s| s.active && s.primary);
    let lit_other = screens.iter().any(|s| s.active && !s.primary);
    let shown = if posture.lights_main() {
        lit_main && !lit_other
    } else {
        lit_other && !lit_main
    };
    if shown {
        Readback::Shown
    } else {
        Readback::NotYet
    }
}

impl SimctlClient {
    /// Fold or unfold the simulator `udid`, and return once the panel the
    /// posture lights is the lit one.
    ///
    /// Nothing appears on screen: the hinge angle goes to the simulator
    /// directly, the way Device Hub's own hinge control sends it.
    ///
    /// # Errors
    ///
    /// [`DeviceControlError::Posture`] when the device has one screen,
    /// when its HID service cannot be reached, or when the lit panel has
    /// not changed within fifteen seconds (naming the panel that is lit).
    ///
    /// Since smix 20.1.0.
    pub async fn set_posture(
        &self,
        udid: &str,
        posture: Posture,
    ) -> Result<(), DeviceControlError> {
        let refuse = |detail: String| DeviceControlError::Posture {
            udid: udid.to_string(),
            posture: posture.name(),
            detail,
        };
        let before = read_screens(udid).await?;
        let already = match read_back(posture, &before) {
            Readback::NotFoldable => {
                return Err(refuse("it has one screen, so it does not fold".into()));
            }
            Readback::Shown => true,
            Readback::NotYet => false,
        };
        self.send_and_hold(udid, posture, already)
            .await
            .map_err(refuse)
    }

    #[cfg(target_os = "macos")]
    async fn send_and_hold(
        &self,
        udid: &str,
        posture: Posture,
        already: bool,
    ) -> Result<(), String> {
        let session = crate::posture_sys::HidSession::open(udid)?;
        let report = crate::posture_sys::Control::Hinge(posture.degrees());
        session.send(&report)?;
        if already {
            tokio::time::sleep(HOLD_WHEN_UNOBSERVABLE).await;
            return Ok(());
        }
        let started = Instant::now();
        let mut sent = started;
        loop {
            let screens = read_screens(udid).await.map_err(|e| e.to_string())?;
            if read_back(posture, &screens) == Readback::Shown {
                return Ok(());
            }
            if started.elapsed() >= SHOWS_WITHIN {
                return Err(format!(
                    "the hinge report was sent and after {} s the device still shows {}",
                    SHOWS_WITHIN.as_secs(),
                    describe(&screens)
                ));
            }
            if sent.elapsed() >= RESEND {
                session.send(&report)?;
                sent = Instant::now();
            }
            tokio::time::sleep(POLL).await;
        }
    }

    #[cfg(not(target_os = "macos"))]
    async fn send_and_hold(&self, _: &str, _: Posture, _: bool) -> Result<(), String> {
        Err("folding a simulator needs macOS".into())
    }
}

/// Which way the lit screen faces, in devicectl's names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Facing {
    /// Upright.
    Portrait,
    /// Upside down.
    PortraitUpsideDown,
    /// Turned so the device's left edge is down.
    LandscapeLeft,
    /// Turned so the device's right edge is down.
    LandscapeRight,
}

impl Facing {
    /// devicectl's `currentDeviceOrientation` for this facing.
    pub fn devicectl_name(self) -> &'static str {
        match self {
            Facing::Portrait => "portrait",
            Facing::PortraitUpsideDown => "portraitUpsideDown",
            Facing::LandscapeLeft => "landscapeLeft",
            Facing::LandscapeRight => "landscapeRight",
        }
    }
}

/// The orientation-picker value that faces the lit screen `facing`.
///
/// The picker names the orientation of the whole device, and devicectl
/// measures against the lit screen, which on an unfolded Duo is the inner
/// panel, a quarter turn from the body. Read on sim-smix-duo (iOS 27.1)
/// 2026-10-31: folded, the names hold as they are; unfolded, `portrait`
/// faces the panel landscape-right, `landscape-left` portrait and
/// `landscape-right` upside down. `portrait-upside-down` changes nothing
/// in either posture, so the facing it would give is out of reach: `None`.
pub fn picker_value(folded: bool, facing: Facing) -> Option<&'static str> {
    match (folded, facing) {
        (true, Facing::Portrait) => Some("portrait"),
        (true, Facing::LandscapeLeft) => Some("landscape-left"),
        (true, Facing::LandscapeRight) => Some("landscape-right"),
        (true, Facing::PortraitUpsideDown) => None,
        (false, Facing::Portrait) => Some("landscape-left"),
        (false, Facing::PortraitUpsideDown) => Some("landscape-right"),
        (false, Facing::LandscapeRight) => Some("portrait"),
        (false, Facing::LandscapeLeft) => None,
    }
}

/// What turning a foldable came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Turned {
    /// The lit screen faces the way asked.
    Done,
    /// The device has one screen; turning it is the runner's business.
    NotFoldable,
    /// devicectl could not say how many screens the device has (an Xcode
    /// without `devicectl device info displays`); the device was not
    /// touched, and turning it is left to the runner as before.
    CannotTell,
}

impl SimctlClient {
    /// Turn a foldable simulator so its lit screen faces `facing`.
    ///
    /// On an iPhone Duo `XCUIDevice.orientation` is taken and nothing
    /// turns, folded or not (measured 2026-10-31: the app's frame and the
    /// panel's rotation stayed put for all four values). Device Hub's
    /// orientation picker does turn it, so this sends what the picker
    /// sends and waits until devicectl reads the facing back.
    ///
    /// A device with one screen returns [`Turned::NotFoldable`] without
    /// touching it, and one devicectl cannot describe [`Turned::CannotTell`].
    ///
    /// # Errors
    ///
    /// [`DeviceControlError::Posture`] when the facing cannot be reached
    /// in the current posture, or does not show within fifteen seconds.
    ///
    /// Since smix 20.1.0.
    pub async fn turn_foldable(
        &self,
        udid: &str,
        facing: Facing,
    ) -> Result<Turned, DeviceControlError> {
        let Ok(screens) = read_screens(udid).await else {
            return Ok(Turned::CannotTell);
        };
        if screens.len() <= 1 {
            return Ok(Turned::NotFoldable);
        }
        let refuse = |detail: String| DeviceControlError::Posture {
            udid: udid.to_string(),
            posture: facing.devicectl_name(),
            detail,
        };
        let folded = screens.iter().any(|s| s.active && s.primary);
        let Some(value) = picker_value(folded, facing) else {
            return Err(refuse(format!(
                "a foldable {} cannot face that way: Device Hub's orientation picker has no setting for it",
                if folded { "folded" } else { "unfolded" }
            )));
        };
        self.send_orientation_and_wait(udid, value, facing)
            .await
            .map_err(refuse)?;
        Ok(Turned::Done)
    }

    #[cfg(target_os = "macos")]
    async fn send_orientation_and_wait(
        &self,
        udid: &str,
        value: &str,
        facing: Facing,
    ) -> Result<(), String> {
        let session = crate::posture_sys::HidSession::open(udid)?;
        let report = crate::posture_sys::Control::Orientation(value);
        session.send(&report)?;
        let started = Instant::now();
        let mut sent = started;
        loop {
            let json = crate::displays::read_displays_json(udid)
                .await
                .map_err(|e| e.to_string())?;
            let now = crate::displays::device_orientation(&json);
            if now.as_deref() == Some(facing.devicectl_name()) {
                return Ok(());
            }
            if started.elapsed() >= SHOWS_WITHIN {
                return Err(format!(
                    "the orientation report `{value}` was sent and after {} s devicectl reads {}",
                    SHOWS_WITHIN.as_secs(),
                    now.as_deref().unwrap_or("no orientation")
                ));
            }
            if sent.elapsed() >= RESEND {
                session.send(&report)?;
                sent = Instant::now();
            }
            tokio::time::sleep(POLL).await;
        }
    }

    #[cfg(not(target_os = "macos"))]
    async fn send_orientation_and_wait(&self, _: &str, _: &str, _: Facing) -> Result<(), String> {
        Err("turning a simulator needs macOS".into())
    }
}

/// Describe `screens` for a refusal: which panel is lit now.
pub fn describe(screens: &[Screen]) -> String {
    let lit: Vec<String> = screens
        .iter()
        .filter(|s| s.active)
        .map(|s| {
            format!(
                "display {} ({})",
                s.id,
                if s.primary { "outer" } else { "inner" }
            )
        })
        .collect();
    if lit.is_empty() {
        "no panel lit".into()
    } else {
        format!("{} lit", lit.join(" and "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::displays::screens;

    const FOLDED: &str = include_str!("../tests/fixtures/displays/duo-folded.json");
    const UNFOLDED: &str = include_str!("../tests/fixtures/displays/duo-unfolded.json");
    const ONE: &str = include_str!("../tests/fixtures/displays/one-panel.json");

    #[test]
    fn closed_is_shown_when_the_outer_panel_is_lit() {
        let folded = screens(FOLDED).unwrap();
        assert_eq!(read_back(Posture::Closed, &folded), Readback::Shown);
        assert_eq!(read_back(Posture::Open, &folded), Readback::NotYet);
        assert_eq!(read_back(Posture::HalfOpen, &folded), Readback::NotYet);
    }

    #[test]
    fn open_and_half_open_are_shown_when_the_inner_panel_is_lit() {
        let unfolded = screens(UNFOLDED).unwrap();
        assert_eq!(read_back(Posture::Open, &unfolded), Readback::Shown);
        assert_eq!(read_back(Posture::HalfOpen, &unfolded), Readback::Shown);
        assert_eq!(read_back(Posture::Closed, &unfolded), Readback::NotYet);
    }

    #[test]
    fn a_one_screen_device_cannot_fold() {
        let one = screens(ONE).unwrap();
        assert_eq!(read_back(Posture::Closed, &one), Readback::NotFoldable);
    }

    #[test]
    fn both_panels_lit_is_not_taken_for_either_posture() {
        let both: Vec<Screen> = screens(UNFOLDED)
            .unwrap()
            .into_iter()
            .map(|s| Screen { active: true, ..s })
            .collect();
        assert_eq!(read_back(Posture::Closed, &both), Readback::NotYet);
        assert_eq!(read_back(Posture::Open, &both), Readback::NotYet);
    }

    #[test]
    fn postures_parse_by_their_names() {
        for p in [Posture::Closed, Posture::HalfOpen, Posture::Open] {
            assert_eq!(p.name().parse::<Posture>(), Ok(p));
        }
        assert!(
            "flat"
                .parse::<Posture>()
                .unwrap_err()
                .contains("closed, half-open or open")
        );
    }

    #[test]
    fn the_picker_value_depends_on_whether_the_device_is_folded() {
        assert_eq!(picker_value(true, Facing::Portrait), Some("portrait"));
        assert_eq!(
            picker_value(false, Facing::Portrait),
            Some("landscape-left")
        );
        assert_eq!(
            picker_value(false, Facing::LandscapeRight),
            Some("portrait")
        );
        assert_eq!(picker_value(true, Facing::PortraitUpsideDown), None);
        assert_eq!(picker_value(false, Facing::LandscapeLeft), None);
    }

    #[test]
    fn the_refusal_names_the_lit_panel() {
        assert_eq!(describe(&screens(FOLDED).unwrap()), "display 1 (outer) lit");
    }
}
