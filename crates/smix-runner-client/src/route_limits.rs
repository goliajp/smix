//! How long each runner route may take, and so how long the host waits.
//!
//! The host used to wait 15 s for every route. Some routes' own waits add
//! up to more than that — a `/back` on iOS tries five strategies with up to
//! 20.5 s of settling between them, and a relaunch is two XCUITest calls
//! that each wait up to 30 s — so on a slow device the host gave up while
//! the runner was still working and reported a step that "may have acted".
//! Each route's longest wait is written here, once; the host's wait is that
//! plus `ANSWER_MARGIN` (15 s). Each runner states the same number beside the
//! route (`// LONGEST WAIT:`), and `runner-waits-fit-the-host` holds the
//! three against each other.
//!
//! A number counts the waits the runner's own code sets — polls, settles,
//! budgets, timeouts — and the platform calls that come with a stated
//! bound. A single XCUITest or UiAutomator query has none; the margin is
//! there for the ones in flight when the route's own waits run out.

use std::time::Duration;

use crate::REQUEST_TIMEOUT;
use crate::input_text::ANSWER_MARGIN;

/// `XCUIApplication.activate()` / `launch()` / `terminate()` each wait for
/// the app up to about 30 s before XCUITest gives up on its own.
pub const XCUI_APP_CALL_MS: u64 = 30_000;

/// One stall of the iOS automation, once per route that queries it. On a
/// GitHub macOS runner one XCUITest existence check inside `/fill` took
/// 17.9 s (t = 17.27 s to 35.18 s in the runner's log) while the queries
/// after it took 0.2 s; doubled for load, as the other rates here are.
/// Counted once per route, not per query: the stall was one event, and the
/// same step's other queries ran at their usual speed.
pub const XCUI_STALL_MS: u64 = 36_000;

/// How long the iOS runner's HTTP server lets any handler run before it
/// answers 500 in the handler's place. It is not a second clock: the host
/// decides how long to wait, and this only has to outlast every such wait.
/// The Swift side states it as `handlerTimeoutSeconds`.
pub const SERVER_HANDLER_TIMEOUT_MS: u64 = 600_000;

/// One synthesized touch: 0.28 s measured on a simulator, doubled for load.
pub const PER_TOUCH_MS: u64 = 600;

/// The runners' gap between a burst's touches when the request names none
/// (`TouchTimeline.defaultIntervalMs`, `TapBurst.INTERVAL_MS`).
pub const BURST_INTERVAL_MS: u32 = 80;

/// The iOS runner's hold per touch when the request names none
/// (`TouchTimeline.defaultHoldMs`).
pub const BURST_HOLD_MS: u32 = 50;

/// How a route's longest wait is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Longest {
    /// A fixed number of milliseconds on this platform.
    Ms(u64),
    /// Set by the request: a typing budget, a dismissal budget, a burst's
    /// length. The method that builds the request works it out.
    FromRequest,
}

/// One route on either runner. `None` where that runner has no such route.
#[derive(Debug, Clone, Copy)]
pub struct Route {
    pub path: &'static str,
    pub ios: Option<Longest>,
    pub android: Option<Longest>,
    /// The iOS handler resolves the target app first, which under
    /// `App-Activate: true` may call `activate()` (at most once per 5 s).
    pub ios_resolves_app: bool,
    /// The iOS handler queries XCUITest, so one stall ([`XCUI_STALL_MS`])
    /// is added to it.
    pub ios_queries_xcui: bool,
}

use Longest::{FromRequest as Req, Ms};

const fn r(
    path: &'static str,
    ios: Option<Longest>,
    android: Option<Longest>,
    ios_resolves_app: bool,
) -> Route {
    Route {
        path,
        ios,
        android,
        ios_resolves_app,
        ios_queries_xcui: ios.is_some() && !BOOKKEEPING.contains_path(path),
    }
}

/// iOS routes that answer from the runner's own state and touch no
/// XCUITest query.
struct Paths(&'static [&'static str]);

impl Paths {
    const fn contains_path(&self, path: &str) -> bool {
        let mut i = 0;
        while i < self.0.len() {
            if const_eq(self.0[i], path) {
                return true;
            }
            i += 1;
        }
        false
    }
}

const fn const_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

const BOOKKEEPING: Paths = Paths(&[
    "/health",
    "/record/start",
    "/record/stop",
    "/record/poll",
    "/diagnostic/dump",
    "/shutdown",
    "/session/close",
    "/session/close-all",
    "/session/list",
]);

const NONE: Option<Longest> = None;
const fn ms(n: u64) -> Option<Longest> {
    Some(Ms(n))
}
const REQ: Option<Longest> = Some(Req);

/// Every route either runner serves. Sources are the handlers' own waits;
/// the runners carry the same numbers beside each route.
pub const ROUTES: &[Route] = &[
    r("/health", ms(0), ms(0), false),
    r("/display", NONE, ms(0), false),
    r("/windows", NONE, ms(0), false),
    r("/coordinate-space", ms(0), NONE, true),
    // Android: the screenshot pacer's longest hold (circuit open, 3 s).
    r("/screenshot", ms(0), ms(3_000), false),
    // iOS: the all-windows walk's 8 s budget.
    r("/tree", ms(8_000), ms(0), true),
    r("/probe", NONE, ms(0), false),
    r("/probe/tree", NONE, ms(0), false),
    r("/find", ms(0), NONE, true),
    // Android: the screenshot pacer (3 s) and the recogniser latch (5 s).
    r("/find-text-by-ocr", ms(0), ms(8_000), true),
    // iOS: the system-popup walk's 11 s budget.
    r("/system-popups", ms(11_000), ms(0), true),
    r("/system-popup-action", ms(0), ms(500), true),
    // iOS: waitForExistence(3 s).
    r("/tap", ms(3_000), NONE, true),
    // iOS: two existence waits (2 s, 3 s) and the 2 s scroll settle.
    // Android: the 1.5 s poll, one lookup's 2 s idle wait that can start
    // just before it ends, a 75 ms pause and a 0.5 s idle wait.
    r("/tap-by-id", ms(7_000), ms(4_075), true),
    r("/tap-at-norm-coord", REQ, REQ, true),
    r("/double-tap-at-norm-coord", NONE, ms(650), false),
    r("/long-press-at-norm-coord", NONE, REQ, false),
    r("/swipe-at-norm-coord", ms(0), ms(500), true),
    r("/swipe-once", ms(0), ms(500), true),
    r("/press-key", ms(0), ms(500), false),
    // iOS: five navigation settles of 2 s, two synthesis waits of 5 s and
    // one fixed 0.5 s settle. Android: the 2 s back settle.
    r("/back", ms(20_500), ms(2_000), true),
    r("/hide-keyboard", REQ, ms(2_500), true),
    r("/input-text", REQ, REQ, false),
    r("/fill", ms(0), NONE, true),
    r("/clear", ms(0), NONE, true),
    // Android: 6 s for focus, 2 s for the field to empty, two 0.5 s idle
    // waits and 2 s for it to empty again after the delete keys.
    r("/clear-text", NONE, ms(11_000), false),
    // iOS: activate(). Android: 0.5 s idle wait and 3 s for the package to
    // come to the front.
    r("/foreground", ms(XCUI_APP_CALL_MS), ms(3_500), false),
    // iOS: a 0.2 s settle. Android: 0.8 s idle wait and 3 s for the
    // rotation to arrive.
    r("/set-orientation", ms(200), ms(3_800), true),
    // Android: the in-app WebView bridge's connect (5 s) and read (6 s).
    r("/webview-eval", NONE, ms(11_000), false),
    r("/record/start", ms(0), ms(0), false),
    r("/record/stop", ms(0), ms(0), false),
    r("/record/poll", ms(0), ms(0), false),
    r("/diagnostic/dump", ms(0), NONE, false),
    r("/shutdown", ms(0), NONE, false),
    // iOS: terminate() then launch().
    r("/soft-cycle", ms(2 * XCUI_APP_CALL_MS), NONE, false),
    r("/session/open", ms(XCUI_APP_CALL_MS), ms(500), false),
    r("/session/close", ms(0), ms(0), false),
    r("/session/close-all", ms(0), ms(0), false),
    r("/session/list", ms(0), ms(0), false),
    r(
        "/session/renew-activation",
        ms(XCUI_APP_CALL_MS),
        ms(500),
        false,
    ),
    r("/session/launch-app", ms(XCUI_APP_CALL_MS), ms(500), false),
    r(
        "/session/terminate-app",
        ms(XCUI_APP_CALL_MS),
        ms(500),
        false,
    ),
    r(
        "/session/relaunch-app",
        ms(2 * XCUI_APP_CALL_MS),
        ms(500),
        false,
    ),
    // Registered only when a resolver is handed to the server; the runner
    // hands none.
    r("/select/resolve", ms(0), NONE, true),
    r("/select/resolve-count", ms(0), NONE, true),
    r("/select/resolve-labels", ms(0), NONE, true),
];

fn route(path: &str) -> Option<&'static Route> {
    ROUTES.iter().find(|r| r.path == path)
}

/// The longer of the two runners' fixed waits for `path`, with the
/// activation an iOS handler may make first. `None` when the request sets
/// the wait, or the path is not a runner route.
fn fixed_longest(path: &str, activates: bool) -> Option<Duration> {
    let r = route(path)?;
    let ios = match r.ios {
        Some(Longest::Ms(n)) => Some(Duration::from_millis(n) + ios_extra(r, activates)),
        Some(Longest::FromRequest) => return None,
        None => None,
    };
    let android = match r.android {
        Some(Longest::Ms(n)) => Some(Duration::from_millis(n)),
        Some(Longest::FromRequest) => return None,
        None => None,
    };
    ios.into_iter().chain(android).max()
}

/// What the iOS side adds to its own waits: an activation first when the
/// host asks for one, and one automation stall.
fn ios_extra(r: &Route, activates: bool) -> Duration {
    let mut extra = Duration::ZERO;
    if activates && r.ios_resolves_app {
        extra += Duration::from_millis(XCUI_APP_CALL_MS);
    }
    if r.ios_queries_xcui {
        extra += Duration::from_millis(XCUI_STALL_MS);
    }
    extra
}

/// How long the host waits for `path`, when the route's own waits are
/// fixed. `None` when the request sets them: the caller passes its own
/// wait from [`wait_for_request`].
pub fn route_wait(path: &str, activates: bool) -> Option<Duration> {
    fixed_longest(path, activates).map(|d| d + ANSWER_MARGIN)
}

/// The host's wait for a route whose longest wait the request sets:
/// `from_request` is that, worked out from the request's own fields.
pub fn wait_for_request(path: &str, from_request: Duration, activates: bool) -> Duration {
    let extra = route(path)
        .filter(|r| r.ios.is_some())
        .map_or(Duration::ZERO, |r| ios_extra(r, activates));
    from_request + extra + ANSWER_MARGIN
}

/// A burst's longest wait: `times` touches `interval_ms` apart, each held
/// `hold_ms`, on the slower runner (Android adds a 0.5 s idle wait).
pub fn burst_longest(times: u32, interval_ms: u32, hold_ms: u32) -> Duration {
    let times = u64::from(times.max(1));
    let span = (times - 1) * u64::from(interval_ms) + times * (u64::from(hold_ms) + PER_TOUCH_MS);
    Duration::from_millis(span + 500)
}

/// The plain wait, for a route with nothing of its own.
pub fn plain_wait() -> Duration {
    REQUEST_TIMEOUT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_route_with_no_waits_keeps_the_plain_wait() {
        assert_eq!(route_wait("/health", false), Some(plain_wait()));
    }

    #[test]
    fn a_back_on_ios_is_waited_for_past_its_settles() {
        let wait = route_wait("/back", false).expect("fixed");
        assert!(
            wait >= Duration::from_millis(20_500) + ANSWER_MARGIN,
            "{wait:?}"
        );
        assert!(wait > plain_wait());
    }

    #[test]
    fn a_relaunch_is_waited_for_through_both_app_calls() {
        let wait = route_wait("/session/relaunch-app", false).expect("fixed");
        assert!(wait >= Duration::from_millis(60_000), "{wait:?}");
    }

    #[test]
    fn activation_is_added_only_where_ios_resolves_the_app() {
        let tree = route_wait("/tree", true).unwrap() - route_wait("/tree", false).unwrap();
        assert_eq!(tree, Duration::from_millis(XCUI_APP_CALL_MS));
        let list = route_wait("/session/list", true).unwrap()
            - route_wait("/session/list", false).unwrap();
        assert_eq!(list, Duration::ZERO);
    }

    #[test]
    fn a_route_the_request_sets_has_no_fixed_wait() {
        assert_eq!(route_wait("/input-text", false), None);
        assert_eq!(route_wait("/tap-at-norm-coord", false), None);
    }

    #[test]
    fn a_burst_is_waited_for_touch_by_touch() {
        let ten = burst_longest(10, 200, 50);
        assert_eq!(
            ten,
            Duration::from_millis(9 * 200 + 10 * (50 + PER_TOUCH_MS) + 500)
        );
        assert!(burst_longest(1, 80, 50) < ten);
    }

    #[test]
    fn a_fill_on_ios_is_waited_for_through_one_automation_stall() {
        // the CI run: one existence check inside /fill took 17.9 s
        let wait = route_wait("/fill", false).expect("fixed");
        assert!(
            wait > Duration::from_millis(17_900) + ANSWER_MARGIN,
            "{wait:?}"
        );
    }

    #[test]
    fn bookkeeping_routes_add_no_stall() {
        assert_eq!(route_wait("/session/list", false), Some(plain_wait()));
        assert_eq!(route_wait("/health", true), Some(plain_wait()));
    }

    #[test]
    fn the_ios_server_outlasts_every_wait_the_host_makes() {
        let server = Duration::from_millis(SERVER_HANDLER_TIMEOUT_MS);
        for r in ROUTES {
            if let Some(wait) = route_wait(r.path, true) {
                assert!(wait < server, "{}: the host waits {wait:?}", r.path);
            }
        }
        let dismiss = wait_for_request(
            "/hide-keyboard",
            crate::hide_keyboard::HIDE_KEYBOARD_BUDGET,
            true,
        );
        assert!(dismiss < server, "{dismiss:?}");
        let burst = wait_for_request("/tap-at-norm-coord", burst_longest(20, 1_000, 500), true);
        assert!(burst < server, "{burst:?}");
    }

    #[test]
    fn every_path_is_listed_once() {
        let mut seen = std::collections::HashSet::new();
        for r in ROUTES {
            assert!(seen.insert(r.path), "{} is listed twice", r.path);
            assert!(
                r.ios.is_some() || r.android.is_some(),
                "{} is on neither runner",
                r.path
            );
        }
    }
}
