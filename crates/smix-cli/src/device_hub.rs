//! Whether Xcode 27's Device Hub is showing a given simulator.
//!
//! Device Hub shows only the simulator selected in it, and it attaches to
//! that one again when it boots: shut the shown simulator down, boot it,
//! and its screen is back in the window within seconds with the selection
//! unchanged (measured 2026-09-29). So "is Device Hub running" and "does it
//! have a window" are not the question a boot asks; "is this UDID the one
//! it shows" is.
//!
//! Reading the answer takes three looks. The process: `DeviceHub.app`'s
//! executable is `DevicesTrampoline`, which starts the real `DeviceHub` and
//! exits, so LaunchServices and System Events hold a dead pid for it (unix
//! id 0) — System Events counts zero windows whatever is on screen, with
//! or without permissions. The pid comes from `pgrep -x DeviceHub`. The
//! windows: the window list, filtered by that pid, needs no permission.
//! The selection: the accessibility tree, which needs the Accessibility
//! permission of whichever app this process is attributed to; without it
//! the answer is "cannot tell", never "no".

/// One Device Hub window as the accessibility tree shows it.
// Only the macOS probe builds these; elsewhere Device Hub does not exist.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HubWindow {
    /// The window title: the shown device's name in the compact
    /// one-device window.
    pub title: String,
    /// UDIDs of the sidebar rows that are selected. Empty in the compact
    /// window, which has no sidebar.
    pub selected: Vec<String>,
    /// Whether any sidebar row was read at all.
    pub has_sidebar: bool,
}

/// What was found about Device Hub and one simulator.
// Only the macOS probe builds these; elsewhere Device Hub does not exist.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HubShows {
    NotRunning,
    NoWindowOnScreen,
    /// A window is on screen and shows another device.
    Other,
    /// A window is on screen and shows this simulator.
    This,
    /// A window is on screen and which device it shows could not be read.
    CannotTell(String),
}

/// Whether a window title names the device `name`. Device Hub titles a
/// window `<name> – <OS> <version>` (read 2026-09-29: `sim-smix-03 – iOS
/// 27.0`); the separator keeps `iPhone 17` from matching `iPhone 17 Pro`.
// Only the macOS probe builds these; elsewhere Device Hub does not exist.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn titles(title: &str, name: &str) -> bool {
    !name.is_empty()
        && (title == name
            || title
                .strip_prefix(name)
                .is_some_and(|rest| rest.starts_with(" – ")))
}

/// Decide from what the accessibility tree gave, for simulator `udid`
/// named `name`.
///
/// A compact window has no sidebar, only a title, and titles are names —
/// which two simulators can share. A title that matches a name another
/// simulator also has is not an answer.
// Only the macOS probe builds these; elsewhere Device Hub does not exist.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn decide(udid: &str, name: &str, name_is_unique: bool, windows: &[HubWindow]) -> HubShows {
    if windows.is_empty() {
        return HubShows::CannotTell(
            "Device Hub has a window on screen and its accessibility tree listed none".into(),
        );
    }
    let mut unsure = None;
    for w in windows {
        if w.has_sidebar {
            if w.selected.iter().any(|s| s.eq_ignore_ascii_case(udid)) {
                return HubShows::This;
            }
        } else if titles(&w.title, name) {
            if name_is_unique {
                return HubShows::This;
            }
            unsure = Some(format!(
                "a compact Device Hub window shows a device named `{name}`, and more than \
                 one simulator has that name"
            ));
        }
    }
    match unsure {
        Some(why) => HubShows::CannotTell(why),
        None => HubShows::Other,
    }
}

/// Look at Device Hub now.
#[cfg(target_os = "macos")]
pub fn probe(udid: &str, name: &str, name_is_unique: bool) -> HubShows {
    let Some(pid) = hub_pid() else {
        return HubShows::NotRunning;
    };
    if sys::on_screen_windows(pid) == 0 {
        return HubShows::NoWindowOnScreen;
    }
    if !sys::accessibility_trusted() {
        return HubShows::CannotTell(
            "Device Hub has a window on screen, and reading which simulator it shows needs \
             the Accessibility permission for the app this command runs under (System \
             Settings > Privacy & Security > Accessibility)"
                .into(),
        );
    }
    decide(udid, name, name_is_unique, &sys::windows(pid))
}

#[cfg(not(target_os = "macos"))]
pub fn probe(_udid: &str, _name: &str, _name_is_unique: bool) -> HubShows {
    HubShows::NotRunning
}

#[cfg(target_os = "macos")]
fn hub_pid() -> Option<i32> {
    let out = std::process::Command::new("pgrep")
        .args(["-x", "DeviceHub"])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .and_then(|l| l.trim().parse().ok())
}

#[cfg(target_os = "macos")]
mod sys {
    use super::HubWindow;
    use std::ffi::c_void;

    type CFTypeRef = *const c_void;

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFArrayGetCount(a: CFTypeRef) -> isize;
        fn CFArrayGetValueAtIndex(a: CFTypeRef, i: isize) -> CFTypeRef;
        fn CFDictionaryGetValue(d: CFTypeRef, key: CFTypeRef) -> CFTypeRef;
        fn CFNumberGetValue(n: CFTypeRef, the_type: isize, out: *mut c_void) -> bool;
        fn CFStringCreateWithBytes(
            alloc: CFTypeRef,
            bytes: *const u8,
            len: isize,
            encoding: u32,
            external: bool,
        ) -> CFTypeRef;
        fn CFStringGetCString(s: CFTypeRef, buf: *mut u8, size: isize, encoding: u32) -> bool;
        fn CFGetTypeID(cf: CFTypeRef) -> usize;
        fn CFStringGetTypeID() -> usize;
        fn CFArrayGetTypeID() -> usize;
        fn CFBooleanGetTypeID() -> usize;
        fn CFBooleanGetValue(b: CFTypeRef) -> bool;
        fn CFRelease(cf: CFTypeRef);
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> CFTypeRef;
        static kCGWindowOwnerPID: CFTypeRef;
        static kCGWindowLayer: CFTypeRef;
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXIsProcessTrusted() -> bool;
        fn AXUIElementCreateApplication(pid: i32) -> CFTypeRef;
        fn AXUIElementCopyAttributeValue(
            el: CFTypeRef,
            attr: CFTypeRef,
            out: *mut CFTypeRef,
        ) -> i32;
        fn AXUIElementSetMessagingTimeout(el: CFTypeRef, seconds: f32) -> i32;
    }

    const UTF8: u32 = 0x0800_0100;
    const SINT64: isize = 4;
    const ON_SCREEN_ONLY: u32 = 1;
    const ROW_PREFIX: &str = "TableRow.Device.";

    /// A CF object this code owns and releases.
    struct Owned(CFTypeRef);
    impl Drop for Owned {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { CFRelease(self.0) }
            }
        }
    }

    fn cfstr(s: &str) -> Owned {
        Owned(unsafe {
            CFStringCreateWithBytes(std::ptr::null(), s.as_ptr(), s.len() as isize, UTF8, false)
        })
    }

    fn string(cf: CFTypeRef) -> Option<String> {
        if cf.is_null() || unsafe { CFGetTypeID(cf) != CFStringGetTypeID() } {
            return None;
        }
        let mut buf = vec![0u8; 1024];
        let ok = unsafe { CFStringGetCString(cf, buf.as_mut_ptr(), buf.len() as isize, UTF8) };
        if !ok {
            return None;
        }
        let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
        Some(String::from_utf8_lossy(&buf[..end]).into_owned())
    }

    fn int(cf: CFTypeRef) -> Option<i64> {
        let mut v: i64 = 0;
        (!cf.is_null() && unsafe { CFNumberGetValue(cf, SINT64, (&mut v as *mut i64).cast()) })
            .then_some(v)
    }

    /// Windows on screen in the normal layer owned by `pid`.
    pub(super) fn on_screen_windows(pid: i32) -> usize {
        let list = Owned(unsafe { CGWindowListCopyWindowInfo(ON_SCREEN_ONLY, 0) });
        if list.0.is_null() {
            return 0;
        }
        let n = unsafe { CFArrayGetCount(list.0) };
        (0..n)
            .map(|i| unsafe { CFArrayGetValueAtIndex(list.0, i) })
            .filter(|w| {
                let owner = int(unsafe { CFDictionaryGetValue(*w, kCGWindowOwnerPID) });
                let layer = int(unsafe { CFDictionaryGetValue(*w, kCGWindowLayer) });
                owner == Some(i64::from(pid)) && layer == Some(0)
            })
            .count()
    }

    pub(super) fn accessibility_trusted() -> bool {
        unsafe { AXIsProcessTrusted() }
    }

    fn attr(el: CFTypeRef, name: &str) -> Option<Owned> {
        let key = cfstr(name);
        let mut out: CFTypeRef = std::ptr::null();
        let err = unsafe { AXUIElementCopyAttributeValue(el, key.0, &mut out) };
        (err == 0 && !out.is_null()).then_some(Owned(out))
    }

    fn elements(el: CFTypeRef, name: &str) -> (Option<Owned>, Vec<CFTypeRef>) {
        let Some(arr) = attr(el, name) else {
            return (None, Vec::new());
        };
        if unsafe { CFGetTypeID(arr.0) != CFArrayGetTypeID() } {
            return (Some(arr), Vec::new());
        }
        let n = unsafe { CFArrayGetCount(arr.0) };
        let items = (0..n)
            .map(|i| unsafe { CFArrayGetValueAtIndex(arr.0, i) })
            .collect();
        (Some(arr), items)
    }

    fn text(el: CFTypeRef, name: &str) -> Option<String> {
        attr(el, name).and_then(|v| string(v.0))
    }

    fn device_under(el: CFTypeRef, depth: usize) -> Option<String> {
        if depth > 4 {
            return None;
        }
        if let Some(id) = text(el, "AXIdentifier")
            && let Some(udid) = id.strip_prefix(ROW_PREFIX)
        {
            return Some(udid.to_string());
        }
        let (_keep, kids) = elements(el, "AXChildren");
        kids.into_iter().find_map(|k| device_under(k, depth + 1))
    }

    fn rows(el: CFTypeRef, depth: usize, found: &mut Vec<(String, bool)>) {
        if depth > 16 {
            return;
        }
        if text(el, "AXRole").as_deref() == Some("AXRow") {
            if let Some(udid) = device_under(el, 0) {
                let selected = attr(el, "AXSelected").is_some_and(|v| unsafe {
                    CFGetTypeID(v.0) == CFBooleanGetTypeID() && CFBooleanGetValue(v.0)
                });
                found.push((udid, selected));
            }
            return;
        }
        let (_keep, kids) = elements(el, "AXChildren");
        for k in kids {
            rows(k, depth + 1, found);
        }
    }

    pub(super) fn windows(pid: i32) -> Vec<HubWindow> {
        let app = Owned(unsafe { AXUIElementCreateApplication(pid) });
        unsafe { AXUIElementSetMessagingTimeout(app.0, 2.0) };
        let (_keep, wins) = elements(app.0, "AXWindows");
        wins.into_iter()
            .map(|w| {
                let mut found = Vec::new();
                rows(w, 0, &mut found);
                HubWindow {
                    title: text(w, "AXTitle").unwrap_or_default(),
                    has_sidebar: !found.is_empty(),
                    selected: found
                        .into_iter()
                        .filter(|(_, s)| *s)
                        .map(|(u, _)| u)
                        .collect(),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: &str = "00000000-0000-4000-8000-000000000003";
    const OTHER: &str = "00000000-0000-4000-8000-000000000004";

    fn main_window(selected: &str) -> HubWindow {
        HubWindow {
            title: String::new(),
            selected: vec![selected.to_string()],
            has_sidebar: true,
        }
    }

    fn compact(title: &str) -> HubWindow {
        HubWindow {
            title: title.to_string(),
            selected: Vec::new(),
            has_sidebar: false,
        }
    }

    #[test]
    fn the_selected_row_is_the_shown_device() {
        assert_eq!(
            decide(ME, "sim-smix-03", true, &[main_window(ME)]),
            HubShows::This
        );
        assert_eq!(
            decide(ME, "sim-smix-03", true, &[main_window(OTHER)]),
            HubShows::Other
        );
    }

    #[test]
    fn a_compact_window_is_read_by_its_title_when_the_name_is_unique() {
        assert_eq!(
            decide(ME, "sim-smix-03", true, &[compact("sim-smix-03")]),
            HubShows::This
        );
        assert_eq!(
            decide(ME, "sim-smix-03", true, &[compact("sim-other")]),
            HubShows::Other
        );
    }

    #[test]
    fn a_title_carries_the_os_after_the_name() {
        assert_eq!(
            decide(
                ME,
                "sim-smix-03",
                true,
                &[compact("sim-smix-03 – iOS 27.0")]
            ),
            HubShows::This
        );
        // A longer name that starts with this one is another device.
        assert_eq!(
            decide(
                ME,
                "iPhone 17",
                true,
                &[compact("iPhone 17 Pro – iOS 27.0")]
            ),
            HubShows::Other
        );
        assert_eq!(
            decide(ME, "", false, &[compact(" – iOS 27.0")]),
            HubShows::Other
        );
    }

    #[test]
    fn a_shared_name_in_a_compact_window_is_not_an_answer() {
        assert!(matches!(
            decide(ME, "iPhone 17", false, &[compact("iPhone 17")]),
            HubShows::CannotTell(_)
        ));
    }

    #[test]
    fn any_window_showing_it_is_enough() {
        assert_eq!(
            decide(
                ME,
                "sim-smix-03",
                true,
                &[main_window(OTHER), compact("sim-smix-03")]
            ),
            HubShows::This
        );
    }

    /// The live instrument: `SMIX_HUB_UDID=<udid> SMIX_HUB_NAME=<name>
    /// cargo test -p smix-cli --bin smix device_hub::tests::what_device_hub_shows_now
    /// -- --ignored --nocapture`. Prints what [`probe`] reads right now.
    #[test]
    #[ignore = "reads this machine's Device Hub"]
    fn what_device_hub_shows_now() {
        let udid = std::env::var("SMIX_HUB_UDID").expect("SMIX_HUB_UDID");
        let name = std::env::var("SMIX_HUB_NAME").unwrap_or_default();
        println!("probe({udid}, {name}) = {:?}", probe(&udid, &name, true));
    }

    #[test]
    fn a_window_on_screen_with_nothing_readable_cannot_tell() {
        assert!(matches!(
            decide(ME, "sim-smix-03", true, &[]),
            HubShows::CannotTell(_)
        ));
    }
}
