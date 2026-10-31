//! The capture path behind `screenshot` and `capture_bgra`: the resident
//! IOSurface host first, `simctl io screenshot` when it cannot answer, and
//! on a device with several screens, the lit one in both.

use crate::displays::{LitScreen, read_lit_screen};
use crate::{DeviceControlError, SimctlClient, surface_capture};

/// Which screen to read, as far as it could be found out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScreenPick {
    /// One screen, or the lit one of several.
    Known(LitScreen),
    /// devicectl did not answer (an Xcode without `info displays`, a device
    /// that is not booted). The direct host cannot be told which framebuffer
    /// to read, so it is skipped; plain `simctl` photographs a one-screen
    /// device and refuses a two-screen one by name.
    Unknown,
}

impl ScreenPick {
    /// The `--display=` value for `simctl io screenshot`, if one is needed.
    pub(crate) fn simctl_display(self) -> Option<u32> {
        match self {
            ScreenPick::Known(LitScreen::Of(id)) => Some(id),
            _ => None,
        }
    }
}

impl SimctlClient {
    /// The screen of `udid` a capture should read.
    ///
    /// A device found to have one screen is remembered, so a phone pays for
    /// the devicectl round trip (about 80 ms) once rather than per frame. A
    /// foldable is asked every time, because folding it moves the picture.
    async fn screen_to_read(&self, udid: &str) -> ScreenPick {
        if self.one_screen.lock().await.contains(udid) {
            return ScreenPick::Known(LitScreen::Only);
        }
        match read_lit_screen(udid).await {
            Ok(LitScreen::Only) => {
                self.one_screen.lock().await.insert(udid.to_string());
                ScreenPick::Known(LitScreen::Only)
            }
            Ok(lit) => ScreenPick::Known(lit),
            Err(_) => ScreenPick::Unknown,
        }
    }

    /// Core capture path: try the resident IOSurface host, fall back to
    /// `simctl`. `want_png` selects an in-host ImageIO PNG encode over a raw
    /// BGRA frame; the fallback is always a PNG.
    pub(crate) async fn capture_frame(
        &self,
        udid: &str,
        want_png: bool,
    ) -> Result<surface_capture::CapturedFrame, DeviceControlError> {
        let pick = self.screen_to_read(udid).await;
        // Direct path first. No pacer gate: the direct IOSurface read does not
        // touch `com.apple.display.captureservice`, so the crash-guard floor
        // the pacer enforces for `simctl io screenshot` does not apply here.
        // Surface unavailable, or the host transport failed — both fall
        // through to the correct-but-slow simctl path below.
        if let ScreenPick::Known(lit) = pick {
            let screen = match lit {
                LitScreen::Only => None,
                LitScreen::Of(id) => Some(id),
            };
            if let Ok(Some(frame)) = self.try_capture_direct(udid, screen, want_png).await {
                return Ok(frame);
            }
        }
        let png = self
            .screenshot_via_simctl(udid, pick.simctl_display())
            .await?;
        Ok(surface_capture::CapturedFrame::Png(png))
    }

    /// Get-or-spawn the resident host for `udid` on `screen` and grab one
    /// frame. Returns `Ok(None)` when the host reports the surface is gone,
    /// `Err` on a transport failure. In both non-`Some` cases the host is
    /// dropped (and killed) so the next call re-resolves from scratch. A
    /// resident host reading another screen is dropped too.
    async fn try_capture_direct(
        &self,
        udid: &str,
        screen: Option<u32>,
        want_png: bool,
    ) -> Result<Option<surface_capture::CapturedFrame>, surface_capture::HostError> {
        // Take the host out from under the lock so a 12.6 MB grab (or a 5s
        // spawn) never serializes captures for other sims.
        let existing = { self.capture_hosts.lock().await.take(udid) };
        let mut host = match existing {
            Some(h) if h.screen == screen => h,
            _ => surface_capture::SurfaceCaptureHost::spawn_on(udid, screen).await?,
        };
        match host.grab(want_png).await {
            Ok(Some(frame)) => {
                self.capture_hosts.lock().await.put(udid, host);
                Ok(Some(frame))
            }
            // Host is exiting (surface gone) — drop it, fall back.
            Ok(None) => Ok(None),
            // Transport died — drop it, fall back.
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_lit_screen_of_several_is_named_to_simctl() {
        assert_eq!(
            ScreenPick::Known(LitScreen::Of(3)).simctl_display(),
            Some(3)
        );
        assert_eq!(ScreenPick::Known(LitScreen::Only).simctl_display(), None);
        assert_eq!(ScreenPick::Unknown.simctl_display(), None);
    }
}
