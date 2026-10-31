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
//! lights reports itself active (measured 0.9–4.7 s).

use crate::displays::{Screen, read_screens};
use crate::{DeviceControlError, SimctlClient};
use std::time::{Duration, Instant};

/// How long a fold may take to show before it is called failed. Measured
/// from the report going out to devicectl calling the other panel lit:
/// 0.9 s to 4.7 s over 26 folds, in two clusters (about 1.4 s and about
/// 4 s), with smix's own part under 0.2 s. A ceiling, not a wait.
const SHOWS_WITHIN: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(100);
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
        session.send_hinge(posture.degrees())?;
        if already {
            tokio::time::sleep(HOLD_WHEN_UNOBSERVABLE).await;
            return Ok(());
        }
        let started = Instant::now();
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
            tokio::time::sleep(POLL).await;
        }
    }

    #[cfg(not(target_os = "macos"))]
    async fn send_and_hold(&self, _: &str, _: Posture, _: bool) -> Result<(), String> {
        Err("folding a simulator needs macOS".into())
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
    fn the_refusal_names_the_lit_panel() {
        assert_eq!(describe(&screens(FOLDED).unwrap()), "display 1 (outer) lit");
    }
}
