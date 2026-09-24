//! What `runner up` does about the screen before it answers: nothing,
//! put a shade away, bring the named app forward, or refuse and say why.
//!
//! Decided here, from what was read; carried out and read back in
//! `runner_android`. The split is the usual one — the part that can be
//! wrong is the part with no device in it.

/// One window as the runner's `/windows` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    /// `AccessibilityWindowInfo.getType()`: 1 is an application window,
    /// 3 a system one.
    pub kind: u64,
    /// The package whose window this is.
    pub package: String,
    /// Whether the runner could read the window's root.
    pub readable: bool,
    /// Whether the window holds the input focus.
    pub focused: bool,
}

/// The screen, as two independent readers see it: the platform's
/// activity manager, and the runner's accessibility connection.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Screen {
    /// The package `dumpsys activity activities` says is resumed, if any.
    pub resumed: Option<String>,
    /// The runner's windows.
    pub windows: Vec<Window>,
}

/// The next thing to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Nothing to put right: answer.
    Ready,
    /// System UI holds the focus and no app can be read: the shade is
    /// over the screen. Put it away and look again.
    CollapseShade,
    /// No app window can be read and nothing is covering the screen: a
    /// window list between two apps. Look again.
    NotYet,
    /// Bring the app the caller named to the front.
    BringForward {
        /// The package the caller named.
        app: String,
        /// What was resumed instead, if anything.
        was: Option<String>,
    },
    /// Nothing smix may do about it; the reason, for the reader.
    Refuse(String),
}

/// Decide the next step.
///
/// `named` is the app the caller asked to have in front (`--bundle`), and
/// `collapsed_already` whether a shade has been put away in this attempt.
#[must_use]
pub fn next_step(screen: &Screen, named: Option<&str>, collapsed_already: bool) -> Step {
    // An app window the runner can read is what "something is in front"
    // means to everything downstream.
    let an_app_can_be_read = screen
        .windows
        .iter()
        .any(|w| w.kind == WINDOW_TYPE_APPLICATION && w.readable);
    if !an_app_can_be_read {
        // Without one, two different screens read the same by window
        // list — both measured as two systemui windows and nothing else.
        // A pulled-down shade has one of them holding the focus; a screen
        // between two apps, a moment after one was started, has neither.
        // Reading the second as the first announced a shade that was
        // never down, and the check after it then reported a crashed
        // instrumentation. Focus is what tells them apart.
        let covered = screen
            .windows
            .iter()
            .any(|w| w.package == SYSTEM_UI && w.focused);
        if !covered {
            return Step::NotYet;
        }
        if !collapsed_already {
            return Step::CollapseShade;
        }
        return Step::Refuse(format!(
            "only system UI is on the screen, and it is still there after putting \
             the notification shade away (windows: {}). Either a lock screen is up \
             — smix does not unlock a device — or the instrumentation crashed and \
             was restarted and sees nothing else",
            describe(&screen.windows),
        ));
    }
    // Only a name the caller gave is ever brought forward. Which app
    // belongs in front is the caller's to say; a package in front is not
    // an owner, and whose device this is was settled by the lease before
    // anything here ran.
    let Some(app) = named else {
        return Step::Ready;
    };
    if screen.resumed.as_deref() == Some(app) {
        // In front means both readers agree: the platform resumes it and
        // the runner can read a window of it. The platform says so first,
        // and a relaunch can leave the stopped instance's window readable
        // for a moment — either alone answered on a screen still changing
        // over.
        let its_window_can_be_read = screen
            .windows
            .iter()
            .any(|w| w.kind == WINDOW_TYPE_APPLICATION && w.readable && w.package == app);
        return if its_window_can_be_read {
            Step::Ready
        } else {
            Step::NotYet
        };
    }
    Step::BringForward {
        app: app.to_string(),
        was: screen.resumed.clone(),
    }
}

/// `AccessibilityWindowInfo.TYPE_APPLICATION`.
const WINDOW_TYPE_APPLICATION: u64 = 1;

/// The package the shade, the status bar and the lock screen belong to.
const SYSTEM_UI: &str = "com.android.systemui";

fn describe(windows: &[Window]) -> String {
    if windows.is_empty() {
        return "none at all".to_string();
    }
    windows
        .iter()
        .map(|w| w.package.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP: u64 = 1;
    const SYSTEM: u64 = 3;

    fn win(kind: u64, package: &str) -> Window {
        Window {
            kind,
            package: package.to_string(),
            readable: true,
            focused: kind == APP,
        }
    }

    fn systemui(focused: bool) -> Window {
        Window {
            focused,
            ..win(SYSTEM, "com.android.systemui")
        }
    }

    fn screen(resumed: Option<&str>, windows: Vec<Window>) -> Screen {
        Screen {
            resumed: resumed.map(str::to_string),
            windows,
        }
    }

    /// The consumer's second shape, as measured on emulator-5554: the
    /// shade pulled down leaves two systemui windows and nothing else,
    /// one of them holding the focus, while the app behind it is still
    /// the resumed one.
    fn shade_down() -> Screen {
        screen(
            Some("dev.smix.fixture"),
            vec![systemui(false), systemui(true)],
        )
    }

    /// What a fresh bring-up read a moment after starting the app,
    /// measured on emulator-5554: the same two systemui windows, and
    /// neither of them holding the focus. By window list alone this is
    /// the shade; by focus it is a screen between two apps.
    fn mid_launch() -> Screen {
        screen(
            Some("dev.smix.fixture"),
            vec![systemui(false), systemui(false)],
        )
    }

    fn home_in_front() -> Screen {
        screen(
            Some("com.android.launcher3"),
            vec![
                win(SYSTEM, "com.android.systemui"),
                win(APP, "com.android.launcher3"),
            ],
        )
    }

    fn other_app_in_front() -> Screen {
        screen(
            Some("com.android.settings"),
            vec![
                win(SYSTEM, "com.android.systemui"),
                win(APP, "com.android.settings"),
            ],
        )
    }

    #[test]
    fn a_shade_over_the_screen_is_put_away_first() {
        assert_eq!(next_step(&shade_down(), None, false), Step::CollapseShade);
        assert_eq!(
            next_step(&shade_down(), Some("dev.smix.fixture"), false),
            Step::CollapseShade,
            "the named app cannot be judged in front while only system UI can be read"
        );
    }

    #[test]
    fn only_system_ui_after_putting_the_shade_away_is_refused_and_says_so() {
        let Step::Refuse(why) = next_step(&shade_down(), None, true) else {
            panic!("expected a refusal once the shade has been put away");
        };
        assert!(
            why.contains("shade") && why.contains("only system UI"),
            "the reason names what was tried and what is still there: {why}"
        );
    }

    #[test]
    fn an_unreadable_app_window_does_not_count_as_one() {
        let mut s = home_in_front();
        for w in &mut s.windows {
            w.readable = false;
        }
        assert_eq!(
            next_step(&s, None, false),
            Step::NotYet,
            "nothing covers the screen, so there is no shade to put away"
        );
    }

    /// A shade is put away only when system UI holds the focus. Without
    /// that, a screen between two apps read as a covered one: the
    /// bring-up announced it had put a shade away that was never down,
    /// and then reported a crashed instrumentation.
    #[test]
    fn a_screen_between_two_apps_is_looked_at_again_not_collapsed() {
        assert_eq!(next_step(&mid_launch(), None, false), Step::NotYet);
        assert_eq!(
            next_step(&mid_launch(), Some("dev.smix.fixture"), false),
            Step::NotYet
        );
        assert_eq!(
            next_step(&mid_launch(), None, true),
            Step::NotYet,
            "after a collapse too: nothing covers it, so there is nothing to refuse"
        );
    }

    #[test]
    fn a_named_app_already_in_front_needs_nothing() {
        let s = screen(
            Some("dev.smix.fixture"),
            vec![
                win(SYSTEM, "com.android.systemui"),
                win(APP, "dev.smix.fixture"),
            ],
        );
        assert_eq!(next_step(&s, Some("dev.smix.fixture"), false), Step::Ready);
    }

    #[test]
    fn a_named_app_behind_the_home_screen_is_brought_forward() {
        assert_eq!(
            next_step(&home_in_front(), Some("dev.smix.fixture"), false),
            Step::BringForward {
                app: "dev.smix.fixture".to_string(),
                was: Some("com.android.launcher3".to_string()),
            }
        );
    }

    #[test]
    fn a_named_app_behind_another_app_is_brought_forward_and_says_which() {
        assert_eq!(
            next_step(&other_app_in_front(), Some("dev.smix.fixture"), false),
            Step::BringForward {
                app: "dev.smix.fixture".to_string(),
                was: Some("com.android.settings".to_string()),
            }
        );
    }

    #[test]
    fn without_a_name_a_readable_app_in_front_is_ready() {
        assert_eq!(next_step(&home_in_front(), None, false), Step::Ready);
        assert_eq!(next_step(&other_app_in_front(), None, false), Step::Ready);
    }

    /// The platform says the app is resumed a moment before the runner
    /// can read its window — and after a relaunch, the runner's list can
    /// still hold a readable window of the instance just stopped. Taking
    /// "resumed" alone as in front let the bring-up answer on a screen
    /// that was still changing over, and the check after it reported a
    /// crashed instrumentation. Measured on emulator-5554.
    #[test]
    fn a_named_app_is_in_front_only_once_its_window_can_be_read() {
        let s = screen(
            Some("dev.smix.fixture"),
            vec![systemui(false), win(APP, "com.android.launcher3")],
        );
        assert_eq!(
            next_step(&s, Some("dev.smix.fixture"), false),
            Step::NotYet,
            "resumed, and no window of it the runner can read yet"
        );
    }

    /// The invariant: smix never decides which app should be in front.
    /// Counted over every screen this file constructs, in both states of
    /// the shade flag, so a screen that stops producing `BringForward`
    /// is not the only thing that could keep this green.
    #[test]
    fn without_a_name_nothing_is_ever_brought_forward() {
        let screens = [
            shade_down(),
            mid_launch(),
            home_in_front(),
            other_app_in_front(),
            Screen::default(),
        ];
        let mut asked = 0;
        let mut brought = 0;
        for s in &screens {
            for collapsed in [false, true] {
                asked += 1;
                if matches!(next_step(s, None, collapsed), Step::BringForward { .. }) {
                    brought += 1;
                }
            }
        }
        assert_eq!(
            asked, 10,
            "the set this counts over is the one written above"
        );
        assert_eq!(brought, 0, "an app nobody named was brought forward");
    }

    /// The same count with a name, so the zero above is not a planner
    /// that never brings anything forward at all.
    #[test]
    fn with_a_name_the_same_screens_do_bring_it_forward() {
        let brought = [home_in_front(), other_app_in_front()]
            .iter()
            .filter(|s| {
                matches!(
                    next_step(s, Some("dev.smix.fixture"), false),
                    Step::BringForward { .. }
                )
            })
            .count();
        assert_eq!(brought, 2);
    }
}
