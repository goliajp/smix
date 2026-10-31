//! Which of a simulator's screens is lit.
//!
//! A foldable has two panels and only one is lit at a time: the outer one
//! folded, the inner one unfolded. `simctl io screenshot` refuses to choose
//! between them ("Cannot choose between 2 displays"), and the framebuffer
//! of the dark panel is all black, so a screenshot has to name the lit one.
//! `devicectl device info displays` says which, per display, as `active`.

use crate::DeviceControlError;
use serde::Deserialize;

/// The screen a capture should read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LitScreen {
    /// The device has one screen; there is nothing to choose.
    Only,
    /// The device has several; this display id is the lit one. It is the
    /// id `simctl io screenshot --display=` takes.
    Of(u32),
}

#[derive(Deserialize)]
struct Envelope {
    result: Option<Result_>,
    error: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct Result_ {
    displays: Vec<Display>,
}

#[derive(Deserialize)]
struct Display {
    #[serde(rename = "displayId")]
    display_id: u32,
    active: bool,
}

/// Read `devicectl device info displays --json-output -` for the lit screen.
pub fn lit_screen(json: &str) -> Result<LitScreen, String> {
    let envelope: Envelope =
        serde_json::from_str(json).map_err(|e| format!("devicectl displays output: {e}"))?;
    let Some(result) = envelope.result else {
        return Err(match envelope.error {
            Some(e) => format!("devicectl could not read the displays: {e}"),
            None => "devicectl displays output has neither a result nor an error".into(),
        });
    };
    if result.displays.len() <= 1 {
        return Ok(LitScreen::Only);
    }
    let lit: Vec<u32> = result
        .displays
        .iter()
        .filter(|d| d.active)
        .map(|d| d.display_id)
        .collect();
    match lit.as_slice() {
        [id] => Ok(LitScreen::Of(*id)),
        [] => Err(format!(
            "none of the {} screens is lit, so there is nothing to photograph",
            result.displays.len()
        )),
        many => Err(format!(
            "{} screens say they are lit ({many:?}); which one to photograph is not clear",
            many.len()
        )),
    }
}

/// Ask devicectl which screen of `udid` is lit.
pub async fn read_lit_screen(udid: &str) -> Result<LitScreen, DeviceControlError> {
    let out = tokio::process::Command::new("xcrun")
        .args([
            "devicectl",
            "device",
            "info",
            "displays",
            "--device",
            udid,
            "--json-output",
            "-",
        ])
        .output()
        .await
        .map_err(|e| DeviceControlError::Malformed {
            subcommand: "screenshot".into(),
            detail: format!("devicectl device info displays: {e}"),
        })?;
    lit_screen(&String::from_utf8_lossy(&out.stdout)).map_err(|detail| {
        DeviceControlError::Malformed {
            subcommand: "screenshot".into(),
            detail,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FOLDED: &str = include_str!("../tests/fixtures/displays/duo-folded.json");
    const UNFOLDED: &str = include_str!("../tests/fixtures/displays/duo-unfolded.json");
    const ONE: &str = include_str!("../tests/fixtures/displays/one-panel.json");

    #[test]
    fn a_folded_duo_is_photographed_on_its_outer_screen() {
        assert_eq!(lit_screen(FOLDED), Ok(LitScreen::Of(1)));
    }

    #[test]
    fn an_unfolded_duo_is_photographed_on_its_inner_screen() {
        assert_eq!(lit_screen(UNFOLDED), Ok(LitScreen::Of(3)));
    }

    #[test]
    fn a_one_screen_device_names_no_screen() {
        assert_eq!(lit_screen(ONE), Ok(LitScreen::Only));
    }

    #[test]
    fn no_lit_screen_is_refused_by_name() {
        let dark = UNFOLDED.replace("\"active\": true", "\"active\": false");
        assert_ne!(
            dark, UNFOLDED,
            "the fixture spells `active` differently than assumed"
        );
        let err = lit_screen(&dark).unwrap_err();
        assert!(err.contains("none of the 2 screens is lit"), "{err}");
    }

    #[test]
    fn a_devicectl_error_is_passed_on() {
        let err = lit_screen(r#"{"error":{"code":1001}}"#).unwrap_err();
        assert!(err.contains("could not read the displays"), "{err}");
    }
}
