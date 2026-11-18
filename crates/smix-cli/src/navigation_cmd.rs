//! `smix sim navigation <device> [three-button|gesture]` — switch an
//! emulator's system navigation, or print the one in use.

use std::time::{Duration, Instant};

/// What the command line said.
#[derive(clap::Args, Debug)]
pub struct NavigationArgs {
    /// Which emulator, by serial or registry alias.
    pub device: String,
    /// `three-button` or `gesture`. Left out, print the navigation in use
    /// and change nothing.
    #[arg(value_parser = parse_mode)]
    pub mode: Option<Navigation>,
}

/// A system navigation mode, as `Settings.Secure.NAVIGATION_MODE` numbers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Navigation {
    ThreeButton,
    /// Only ever read: Android 12 took it out of Settings, so smix does
    /// not switch to it.
    TwoButton,
    Gesture,
}

impl Navigation {
    pub fn as_str(self) -> &'static str {
        match self {
            Navigation::ThreeButton => "three-button",
            Navigation::TwoButton => "two-button",
            Navigation::Gesture => "gesture",
        }
    }

    fn overlay(self) -> &'static str {
        match self {
            Navigation::ThreeButton => "com.android.internal.systemui.navbar.threebutton",
            Navigation::TwoButton => "com.android.internal.systemui.navbar.twobutton",
            Navigation::Gesture => "com.android.internal.systemui.navbar.gestural",
        }
    }
}

pub fn parse_mode(s: &str) -> Result<Navigation, String> {
    match s.to_ascii_lowercase().as_str() {
        "three-button" => Ok(Navigation::ThreeButton),
        "gesture" => Ok(Navigation::Gesture),
        other => Err(format!(
            "expected 'three-button' or 'gesture', got {other:?}"
        )),
    }
}

pub async fn run(args: NavigationArgs) -> Result<(), crate::CliError> {
    let serial = crate::resolve_android_serial(&args.device)?;
    let Some(mode) = args.mode else {
        let now = read_mode(&serial).await.map_err(crate::CliError::Other)?;
        println!("navigation: {serial} {}", now.as_str());
        return Ok(());
    };
    let control = smix_sdk::android_device::AndroidDeviceControl::new();
    let s = serial.clone();
    let was = crate::with_device_lease(&control, &serial, |leased| async move {
        let was = switch(&s, mode).await.map_err(crate::CliError::Other)?;
        Ok((was, leased))
    })
    .await?;
    if was == mode {
        println!("navigation: {serial} was already {}", mode.as_str());
    } else {
        println!("navigation: {serial} {} → {}", was.as_str(), mode.as_str());
    }
    Ok(())
}

async fn read_mode(serial: &str) -> Result<Navigation, String> {
    let out = smix_adb::AdbClient::new()
        .shell(serial, &["settings", "get", "secure", "navigation_mode"])
        .await
        .map_err(|e| e.to_string())?;
    mode_of_setting(&out).ok_or_else(|| {
        format!(
            "{serial}: navigation_mode reads {:?}, not 0, 1 or 2 — Android 10 and later keep one",
            out.trim()
        )
    })
}

/// Switches and returns the mode it found, once both the setting and the
/// window manager's navigation bar have changed: SystemUI writes the
/// setting about 0.1 s before the bar is laid out again (measured on a
/// phone and a tablet emulator, API 36).
async fn switch(serial: &str, mode: Navigation) -> Result<Navigation, String> {
    let adb = smix_adb::AdbClient::new();
    let was = read_mode(serial).await?;
    if was == mode {
        return Ok(was);
    }
    let bar = |out: String| bar_height(&out);
    let before = bar(dumpsys_window(&adb, serial).await?);
    adb.shell(
        serial,
        &[
            "cmd",
            "overlay",
            "enable-exclusive",
            "--category",
            mode.overlay(),
        ],
    )
    .await
    .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let (seen, height) = loop {
        // a read that began before the deadline still counts
        let looked = Instant::now();
        let seen = read_mode(serial).await?;
        let height = bar(dumpsys_window(&adb, serial).await?);
        if seen == mode && height.is_some() && height != before {
            return Ok(was);
        }
        if looked >= deadline {
            break (seen, height);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let px = |h: Option<i32>| h.map_or("no navigation bar".to_string(), |h| format!("{h} px"));
    Err(format!(
        "{serial}: asked for {} navigation, but after 10 s the setting reads {} and the \
         navigation bar is {} (it was {} before) — \
         `adb -s {serial} shell cmd overlay list` shows which navbar overlay is on",
        mode.as_str(),
        seen.as_str(),
        px(height),
        px(before),
    ))
}

async fn dumpsys_window(adb: &smix_adb::AdbClient, serial: &str) -> Result<String, String> {
    adb.shell(serial, &["dumpsys", "window"])
        .await
        .map_err(|e| e.to_string())
}

/// `settings get secure navigation_mode` — `null` where the key was never
/// written.
fn mode_of_setting(out: &str) -> Option<Navigation> {
    match out.trim() {
        "0" => Some(Navigation::ThreeButton),
        "1" => Some(Navigation::TwoButton),
        "2" => Some(Navigation::Gesture),
        _ => None,
    }
}

/// Height in pixels of the first `type=navigationBars` inset source in
/// `dumpsys window`, e.g. `type=navigationBars frame=[0,2208][1080,2340]`.
fn bar_height(out: &str) -> Option<i32> {
    let at = out.find("type=navigationBars frame=[")?;
    let rest = &out[at + "type=navigationBars frame=[".len()..];
    let (top_left, rest) = rest.split_once("][")?;
    let (bottom_right, _) = rest.split_once(']')?;
    let top: i32 = top_left.split_once(',')?.1.parse().ok()?;
    let bottom: i32 = bottom_right.split_once(',')?.1.parse().ok()?;
    Some(bottom - top)
}

#[cfg(test)]
mod tests {
    use super::*;

    // read off emulator-5554 (sim-smix-android-01, API 36) in each mode
    const GESTURE: &str = "    mHasBottomNavigationBar=true\n        InsetsSource id=ed220001 type=navigationBars frame=[0,2274][1080,2340] visible=true flags=SUPPRESS_SCRIM|ANIMATE_RESIZING sideHint=BOTTOM boundingRects=null\n        InsetsSource id=ed220001 type=navigationBars frame=[0,0][0,0] visible=false\n";
    const THREE: &str = "        InsetsSource id=ed220001 type=navigationBars frame=[0,2208][1080,2340] visible=true flags=SUPPRESS_SCRIM|ANIMATE_RESIZING sideHint=BOTTOM boundingRects=null\n";

    #[test]
    fn the_bar_height_comes_from_the_first_navigation_bar_source() {
        assert_eq!(bar_height(GESTURE), Some(66));
        assert_eq!(bar_height(THREE), Some(132));
        assert_eq!(bar_height("type=statusBars frame=[0,0][1080,136]\n"), None);
        assert_eq!(bar_height("type=navigationBars frame=[0,2208]["), None);
    }

    #[test]
    fn the_setting_reads_as_its_three_modes_and_nothing_else() {
        assert_eq!(mode_of_setting("0\n"), Some(Navigation::ThreeButton));
        assert_eq!(mode_of_setting("1\n"), Some(Navigation::TwoButton));
        assert_eq!(mode_of_setting("2\n"), Some(Navigation::Gesture));
        assert_eq!(mode_of_setting("null\n"), None);
        assert_eq!(mode_of_setting("3\n"), None);
    }

    #[test]
    fn only_the_two_offered_modes_parse() {
        assert_eq!(parse_mode("Three-Button"), Ok(Navigation::ThreeButton));
        assert_eq!(parse_mode("gesture"), Ok(Navigation::Gesture));
        assert!(parse_mode("two-button").is_err());
        assert!(parse_mode("3").is_err());
    }
}
