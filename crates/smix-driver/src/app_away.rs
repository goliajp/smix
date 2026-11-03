//! A poll whose app is not in front: how long it keeps looking, and, on
//! Android, how a tree says the app has no window on screen.

use std::time::{Duration, Instant};

use smix_error::{ExpectationFailure, FailureCode, FailureInit};
use smix_screen::{A11yNode, WindowKind};

/// How long a poll keeps looking while the app it reads is not in front:
/// long enough for a notification a step just sent to show, so a tap on it
/// is not refused first (measured on an Android 16 emulator: the heads-up
/// was in the tree 0.3 s after `sendPush`, once 1.95 s), and for an app a
/// tapped notification is bringing back. Past it nothing in flight is
/// bringing the app back, and waiting out the whole budget would only
/// delay the same answer.
pub(crate) const APP_RETURNS_WITHIN: Duration = Duration::from_secs(3);

/// Since when a poll has seen its app away, without a reading of it in
/// between.
#[derive(Default)]
pub(crate) struct Away(Option<Instant>);

impl Away {
    /// Whether the app has now been away for [`APP_RETURNS_WITHIN`];
    /// the first sight starts the clock.
    pub(crate) fn for_long_enough(&mut self) -> bool {
        self.0.get_or_insert_with(Instant::now).elapsed() >= APP_RETURNS_WITHIN
    }

    /// [`Self::for_long_enough`] for a failure: only `APP_NOT_IN_FOREGROUND`
    /// counts.
    pub(crate) fn gives_up(&mut self, e: &ExpectationFailure) -> bool {
        e.code == FailureCode::AppNotInForeground && self.for_long_enough()
    }

    pub(crate) fn seen(&mut self) {
        self.0 = None;
    }
}

/// The app windows an Android tree shows in place of `app`'s, or `None`
/// when `app` has a window in it — or when a window could not be read,
/// since that one may be the app's.
pub(crate) fn app_windows_instead(tree: &A11yNode, app: &str) -> Option<Vec<String>> {
    if tree.unreadable_windows.unwrap_or(0) > 0 {
        return None;
    }
    let mut instead: Vec<String> = Vec::new();
    for w in tree.children.iter().filter_map(|c| c.window.as_ref()) {
        let Some(pkg) = w.package.as_deref() else {
            return None;
        };
        if pkg == app {
            return None;
        }
        if w.kind == WindowKind::Application && !instead.iter().any(|p| p == pkg) {
            instead.push(pkg.to_string());
        }
    }
    Some(instead)
}

/// An Android poll's check after a miss: the failure once `app` has had no
/// window on screen for [`APP_RETURNS_WITHIN`], `None` until then.
pub(crate) fn check(
    app: Option<&str>,
    tree: &A11yNode,
    away: &mut Away,
) -> Option<ExpectationFailure> {
    let Some((app, instead)) = app.and_then(|a| app_windows_instead(tree, a).map(|i| (a, i)))
    else {
        away.seen();
        return None;
    };
    away.for_long_enough().then(|| not_in_front(app, &instead))
}

/// The failure for `app` having no window on screen.
pub(crate) fn not_in_front(app: &str, instead: &[String]) -> ExpectationFailure {
    let front = match instead {
        [] => "no app window is on screen (the home screen)".to_string(),
        [one] => format!("{one} is in front"),
        many => format!("{} are in front", many.join(", ")),
    };
    ExpectationFailure::new(FailureInit {
        code: Some(FailureCode::AppNotInForeground),
        message: format!(
            "{app} has no window on screen — it is in the background, or not running; {front}. \
             Bring it back with launchApp, or tap the notification that opens it."
        ),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use smix_screen::WindowInfo;

    fn failure(code: FailureCode) -> ExpectationFailure {
        ExpectationFailure::new(FailureInit {
            code: Some(code),
            ..Default::default()
        })
    }

    #[test]
    fn only_an_app_that_stayed_away_ends_a_poll() {
        let away = failure(FailureCode::AppNotInForeground);
        let mut clock = Away::default();
        assert!(!clock.gives_up(&away), "the first sight starts the clock");
        assert!(!clock.gives_up(&failure(FailureCode::DriverError)));
        clock.0 = Some(Instant::now() - APP_RETURNS_WITHIN);
        assert!(clock.gives_up(&away), "a second later it ends the poll");
        clock.seen();
        assert!(
            !clock.gives_up(&away),
            "a reading in between starts it again"
        );
    }

    fn window(pkg: Option<&str>, kind: WindowKind) -> A11yNode {
        let mut n: A11yNode =
            serde_json::from_str(r#"{"rawType":"x","elementTypeRaw":0,"enabled":true,"selected":false,"hasFocus":false,"visible":true,"bounds":{"x":0,"y":0,"w":1,"h":1}}"#).unwrap();
        n.window = Some(WindowInfo {
            package: pkg.map(String::from),
            kind,
            focused: false,
            layer: None,
            touchable: None,
            touchable_rects: vec![],
        });
        n
    }

    fn screen(windows: Vec<A11yNode>, unreadable: u32) -> A11yNode {
        let mut root: A11yNode =
            serde_json::from_str(r#"{"rawType":"x","elementTypeRaw":0,"enabled":true,"selected":false,"hasFocus":false,"visible":true,"bounds":{"x":0,"y":0,"w":1,"h":1}}"#).unwrap();
        root.children = windows;
        root.unreadable_windows = Some(unreadable);
        root
    }

    #[test]
    fn an_app_with_a_window_is_not_away() {
        let t = screen(
            vec![
                window(Some("com.android.systemui"), WindowKind::System),
                window(Some("com.example.shop"), WindowKind::Application),
            ],
            0,
        );
        assert_eq!(app_windows_instead(&t, "com.example.shop"), None);
    }

    #[test]
    fn another_app_in_front_is_named_and_the_chrome_is_not() {
        let t = screen(
            vec![
                window(Some("com.android.systemui"), WindowKind::System),
                window(Some("com.android.settings"), WindowKind::Application),
            ],
            0,
        );
        assert_eq!(
            app_windows_instead(&t, "com.example.shop"),
            Some(vec!["com.android.settings".to_string()])
        );
        let e = not_in_front("com.example.shop", &["com.android.settings".to_string()]);
        assert_eq!(e.code, FailureCode::AppNotInForeground);
        assert!(
            e.message.contains("com.android.settings is in front"),
            "{}",
            e.message
        );
    }

    #[test]
    fn a_window_that_could_not_be_read_or_named_says_nothing() {
        let t = screen(
            vec![window(
                Some("com.android.settings"),
                WindowKind::Application,
            )],
            1,
        );
        assert_eq!(app_windows_instead(&t, "com.example.shop"), None);
        let t = screen(vec![window(None, WindowKind::Application)], 0);
        assert_eq!(app_windows_instead(&t, "com.example.shop"), None);
    }
}
