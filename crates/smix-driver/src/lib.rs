//! smix-driver — decide layer (CLAUDE.md §12.1 middle).
//!
//! Wraps [`HttpRunnerClient`] (sense + act IPC) with host-side resolve
//! dispatch. The default path is: SDK call → `driver.tap` → `tree()` →
//! `resolve_selector()` → centroid → `runner.tap_at_norm_coord()`
//! (Apple native event chain).
//!
//! # Failure model
//!
//! All methods return `Result<T, ExpectationFailure>` — AI-readable
//! rendering lives in `smix-error`.
//!
//! # Implicit wait
//!
//! `tap` includes a 5s poll-and-retry loop: if the first
//! `resolve_selector` returns None, sleep 250 ms then re-fetch tree +
//! re-resolve, up to a 5s total budget. Once a candidate is found we
//! tap **the same frame** to avoid a split-fetch race.

#![doc(html_root_url = "https://docs.smix.dev/smix-driver")]

use smix_error::{ExpectationFailure, FailureCode, FailureInit};
use smix_host_coord_resolver::{HostResolveError, resolve_to_norm_coord};
use smix_input::{KeyName, SwipeDirection};
use smix_screen::{
    A11yNode, DEFAULT_VISIBLE_LIMIT, ScreenDescription, collect_visible_summaries, summarize_node,
};
use smix_selector::{Modifiers, Pattern, Selector, True, describe_selector, match_text_compiled};
use smix_selector_resolver::{
    ResolverContext, resolve_selector, resolve_selector_all, resolve_selector_compiled,
};
use std::time::{Duration, Instant};
use tokio::time::sleep;

/// Sim device orientation. Driver-level type; SDK exposes a 1:1
/// `MaestroOrientation` mirror for maestro yaml literal alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    /// Standard upright portrait.
    Portrait,
    /// Upside-down portrait (home indicator at top).
    PortraitUpsideDown,
    /// Landscape with home indicator to the right.
    LandscapeLeft,
    /// Landscape with home indicator to the left.
    LandscapeRight,
}

impl Orientation {
    /// Wire literal sent to swift handler (`XCUIDevice.shared.orientation`
    /// switch mapping).
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Portrait => "portrait",
            Self::PortraitUpsideDown => "portraitUpsideDown",
            Self::LandscapeLeft => "landscapeLeft",
            Self::LandscapeRight => "landscapeRight",
        }
    }
}

pub use smix_runner_client::{
    HttpRunnerClient, IncludeScope, OcrFrame, RunnerScrollSelector, RunnerTransportError,
    SystemPopup, TapMode,
};

const POLL_INTERVAL_MS: u64 = 250;
const TOTAL_TIMEOUT_MS: u64 = 5000;
const SCROLL_MAX_SWIPES: u32 = 30;

/// Driver wrapping `HttpRunnerClient` with host-side resolve dispatch.
///
/// Called `IosDriver` to make the platform explicit in the
/// cross-platform Driver trait architecture. The type alias
/// `pub type SimctlDriver = IosDriver` (in `lib.rs`) keeps existing
/// callers working without source edits.
pub struct IosDriver {
    runner: HttpRunnerClient,
}

impl IosDriver {
    pub fn new(runner: HttpRunnerClient) -> Self {
        IosDriver { runner }
    }

    pub fn runner(&self) -> &HttpRunnerClient {
        &self.runner
    }

    /// Mutable accessor for the wrapped client. Used by the
    /// Driver-trait pass-through (`set_target_bundle_id` /
    /// `set_auto_activate`) so the client's per-request context can be
    /// mutated after driver construction.
    pub fn runner_mut(&mut self) -> &mut HttpRunnerClient {
        &mut self.runner
    }

    /// Set the target bundle id sent to the runner on every request.
    /// Threads `--bundle-id` from the CLI down to the wire so the
    /// runner's per-request rebind logic can resolve to the right
    /// XCUIApplication.
    #[must_use]
    pub fn with_target_bundle_id<S: Into<String>>(mut self, bundle: S) -> Self {
        self.runner = self.runner.with_target_bundle_id(bundle);
        self
    }

    /// Enable `App-Activate: true` on every request. Forces the runner
    /// to call `.activate()` on the resolved target before each
    /// operation. Wired from `smix run --activate`.
    #[must_use]
    pub fn with_auto_activate(mut self, activate: bool) -> Self {
        self.runner = self.runner.with_auto_activate(activate);
        self
    }

    // ---- sense ---------------------------------------------------------

    /// Fetch full a11y tree (passthru to `GET /tree`).
    pub async fn tree(
        &self,
        include: Option<IncludeScope>,
    ) -> Result<A11yNode, ExpectationFailure> {
        self.runner
            .get_tree(include)
            .await
            .map_err(transport_to_failure)
    }

    /// Aggregate `ScreenDescription` (DFS visible summaries; no
    /// screenshot here — caller adds when needed).
    pub async fn describe(&self) -> Result<ScreenDescription, ExpectationFailure> {
        let tree = self.tree(None).await?;
        Ok(ScreenDescription {
            screenshot: None,
            elements: collect_visible_summaries(&tree, DEFAULT_VISIBLE_LIMIT),
            front_app: String::new(),
            summary: String::new(),
            captured_at: 0.0,
        })
    }

    /// Resolve selector → single node (passthru-ish: driver.tree + resolver).
    pub async fn find_one(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<Option<A11yNode>, ExpectationFailure> {
        let tree = self.tree_with_retry(include).await?;
        Ok(resolve_selector(&tree, selector).cloned())
    }

    /// Resolve selector → its centroid as viewport-normalized
    /// `(nx, ny)` in `[0, 1]`. `None` when selector resolves no node OR
    /// the matched node has an empty / offscreen frame. Used by adapter
    /// AnchorRelative dispatch to find an anchor's center before adding
    /// a `(dx, dy)` shift.
    pub async fn find_norm_coord(
        &self,
        selector: &Selector,
    ) -> Result<Option<(f64, f64)>, ExpectationFailure> {
        let tree = self.tree_with_retry(None).await?;
        match resolve_to_norm_coord(&tree, selector) {
            Ok((nx, ny)) => Ok(Some((nx, ny))),
            Err(HostResolveError::NotFound | HostResolveError::EmptyMatchedFrame) => Ok(None),
            Err(HostResolveError::UnknownAppFrame) => Err(ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: "find_norm_coord: tree bounds w/h ≤ 0 (unknown app frame)".into(),
                ..Default::default()
            })),
            Err(HostResolveError::CentroidOutOfFrame { .. }) => Ok(None),
        }
    }

    /// Resolve selector → all matching nodes.
    pub async fn find_all(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<Vec<A11yNode>, ExpectationFailure> {
        let tree = self.tree_with_retry(include).await?;
        Ok(resolve_selector_all(&tree, selector)
            .into_iter()
            .cloned()
            .collect())
    }

    /// Boolean existence quick-probe.
    ///
    /// Selector-type dispatch:
    /// - Text selectors with no spatial / index modifiers → runner
    ///   `/find` route (Apple element query, no full-tree snapshot —
    ///   the fast path).
    /// - Id / Label / Role / Focused / Anchor selectors, and Text
    ///   with spatial (`below` / `near` / ...) or index (`nth` /
    ///   `first` / `last`) modifiers → host-resolve fallback:
    ///   `tree() + resolve_selector_all()` client-side. The runner
    ///   `/find` route only knows how to query by text predicate, so
    ///   anything richer has to ride on the full tree snapshot.
    ///
    /// Both dispatch branches poll within `TOTAL_TIMEOUT_MS` on
    /// `/find` 500 or `/tree` 500 (sim still launching, runner
    /// re-attach, etc.), matching the `wait_for` transient-retry
    /// pattern.
    pub async fn find(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<bool, ExpectationFailure> {
        if can_use_find_route(selector) {
            let start = Instant::now();
            let timeout = Duration::from_millis(TOTAL_TIMEOUT_MS);
            let mut last_transport_err: Option<ExpectationFailure> = None;
            loop {
                // v1.0.27 — the live route asks for on-screen (frame ∩
                // app frame), not bare existence. Bare `.exists` is
                // true for below-the-fold elements, which made `find`
                // (and everything built on it: runFlow.when gates,
                // wait_for_not_visible, tapOn poll probes) disagree
                // with tapOn's honest resolution on scrollable screens.
                match self.runner.find_on_screen(selector, include).await {
                    Ok(present) => return Ok(present),
                    Err(e) => {
                        let failure = transport_to_failure(e);
                        if start.elapsed() >= timeout {
                            return Err(last_transport_err.unwrap_or(failure));
                        }
                        last_transport_err = Some(failure);
                    }
                }
                sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
            }
        } else {
            let tree = self.tree_with_retry(include).await?;
            // v1.0.27 — tree-resolve branch gains the same live
            // on-screen confirmation as wait_for. See
            // `confirm_on_screen` for semantics.
            let matched = resolve_selector_all(&tree, selector);
            if matched.is_empty() {
                return Ok(false);
            }
            Ok(self.confirm_on_screen(&matched, include).await)
        }
    }

    /// v1.0.27 — live on-screen confirmation for tree-matched nodes.
    ///
    /// iOS 26.5 + RN 0.86 Fabric SNAPSHOT frames drift for
    /// below-the-fold elements: the tree reports stale in-viewport
    /// coords with `visible=true`, so the resolver's frame∩viewport
    /// filter passes while the element is actually off screen. The
    /// LIVE XCUI query re-resolves current layout and tells the
    /// truth (the same reason `tapOn` fails honestly on such
    /// elements).
    ///
    /// For up to the first 3 matched nodes that carry a
    /// live-queryable handle (`identifier`, else `label` — the two
    /// fields the runner `/find` predicate matches), ask the runner
    /// whether an element with that handle is on screen right now.
    /// Any confirmed node ⇒ true. Nodes with NO handle can't be
    /// live-confirmed — if none of the matched nodes has a handle,
    /// the tree verdict stands (pre-v1.0.27 semantics; OCR tiers
    /// remain the fallback for handle-less degraded trees).
    ///
    /// Transport errors during confirmation also let the tree
    /// verdict stand: a flaky live probe must not turn a legitimate
    /// tree hit into a miss.
    async fn confirm_on_screen(
        &self,
        matched: &[&A11yNode],
        include: Option<IncludeScope>,
    ) -> bool {
        let mut had_handle = false;
        for node in matched.iter().take(3) {
            let handle = node
                .identifier
                .as_deref()
                .filter(|s| !s.is_empty())
                .or_else(|| node.label.as_deref().filter(|s| !s.is_empty()));
            let Some(handle) = handle else { continue };
            had_handle = true;
            let probe = Selector::Text {
                text: Pattern::Text(handle.to_string()),
                modifiers: smix_selector::Modifiers::default(),
            };
            match self.runner.find_on_screen(&probe, include).await {
                Ok(true) => return true,
                Ok(false) => continue,
                // Live probe unavailable → tree verdict stands.
                Err(_) => return true,
            }
        }
        // No node carried a live handle → cannot confirm → trust tree.
        !had_handle
    }

    /// Transient `/tree` transport retry helper. Shared by
    /// `find_one` / `find_all` / `find` (host-resolve branch). Mirrors
    /// the transport-retry segment of `wait_for`: poll within
    /// `TOTAL_TIMEOUT_MS` budget, sleep `POLL_INTERVAL_MS` between
    /// attempts, surface only the last transport error on budget
    /// exhaustion.
    ///
    /// Not folded into `wait_for` / `tap` because those loops have
    /// additional in-loop logic (selector poll + ctx cache for
    /// `wait_for`; `HostResolveError::NotFound` retry for `tap`) — DRY
    /// here would harm clarity.
    async fn tree_with_retry(
        &self,
        include: Option<IncludeScope>,
    ) -> Result<A11yNode, ExpectationFailure> {
        let start = Instant::now();
        let timeout = Duration::from_millis(TOTAL_TIMEOUT_MS);
        let mut last_transport_err: Option<ExpectationFailure> = None;
        loop {
            match self.tree(include).await {
                Ok(tree) => return Ok(tree),
                Err(e) => {
                    if start.elapsed() >= timeout {
                        return Err(last_transport_err.unwrap_or(e));
                    }
                    last_transport_err = Some(e);
                }
            }
            sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
        }
    }

    /// `GET /system-popups` passthru.
    pub async fn system_popups(
        &self,
        include: Option<IncludeScope>,
    ) -> Result<Vec<SystemPopup>, ExpectationFailure> {
        self.runner
            .system_popups(include)
            .await
            .map_err(transport_to_failure)
    }

    /// `POST /system-popup-action` passthru.
    /// Returns `Ok(true)` when the runner matched and tapped, `Ok(false)`
    /// on 404 not_found. Transport errors map to `ExpectationFailure`.
    pub async fn system_popup_action(
        &self,
        popup_id: &str,
        button_id: &str,
    ) -> Result<bool, ExpectationFailure> {
        self.runner
            .system_popup_action(popup_id, button_id)
            .await
            .map_err(transport_to_failure)
    }

    // ---- act -----------------------------------------------------------

    /// Tap a selector. Host-side resolve → centroid →
    /// `runner.tap_at_norm_coord` (Apple native event chain), with a
    /// 5s implicit wait + retry loop.
    ///
    /// The resolve+centroid+normalize pipeline lives in the
    /// [`smix_host_coord_resolver`] stone; this method orchestrates the
    /// smix-specific 5s implicit-wait loop plus AI-readable failure
    /// rendering + runner injection.
    pub async fn tap(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<(), ExpectationFailure> {
        let start = Instant::now();
        let timeout = Duration::from_millis(TOTAL_TIMEOUT_MS);

        let (nx, ny) = loop {
            // Transport retry parity with wait_for / find. Tree fetch
            // transient transport drops (runner socket refusal /
            // concurrent-handling hiccup) are re-tried in-loop.
            let tree = self.tree_with_retry(include).await?;
            match resolve_to_norm_coord(&tree, selector) {
                Ok(coord) => break coord,
                Err(HostResolveError::NotFound) => {
                    if start.elapsed() > timeout {
                        let visible = collect_visible_summaries(&tree, 10);
                        let target = base_text_or_id(selector);
                        let suggestions =
                            smix_error::build_suggestions(target.as_deref(), &visible);
                        return Err(ExpectationFailure::new(FailureInit {
                            code: Some(FailureCode::ElementNotFound),
                            message: format!(
                                "element not found: {}",
                                describe_selector(selector)
                            ),
                            selector: Some(selector.clone()),
                            visible_elements: visible,
                            suggestions,
                            hint: Some(
                                "matched 0 nodes in the current a11y tree; check selector or wait for the screen to settle"
                                    .into(),
                            ),
                            ..Default::default()
                        }));
                    }
                    sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
                    continue;
                }
                Err(HostResolveError::EmptyMatchedFrame) => {
                    return Err(ExpectationFailure::new(FailureInit {
                        code: Some(FailureCode::ElementNotFound),
                        message: format!(
                            "matched node has empty/offscreen frame: {}",
                            describe_selector(selector)
                        ),
                        selector: Some(selector.clone()),
                        hint: Some(
                            "node bounds w*h == 0; element may be offscreen or hidden".into(),
                        ),
                        ..Default::default()
                    }));
                }
                Err(HostResolveError::UnknownAppFrame) => {
                    return Err(ExpectationFailure::new(FailureInit {
                        code: Some(FailureCode::DriverError),
                        message: format!(
                            "tree bounds w/h ≤ 0 — unknown app frame: {}",
                            describe_selector(selector)
                        ),
                        selector: Some(selector.clone()),
                        hint: Some(
                            "runner returned a tree with empty app frame; app may not be foregrounded"
                                .into(),
                        ),
                        ..Default::default()
                    }));
                }
                Err(HostResolveError::CentroidOutOfFrame { nx, ny }) => {
                    return Err(ExpectationFailure::new(FailureInit {
                        code: Some(FailureCode::ElementNotFound),
                        message: format!(
                            "matched node centroid out of app frame: {}",
                            describe_selector(selector)
                        ),
                        selector: Some(selector.clone()),
                        hint: Some(format!(
                            "centroid (nx={:.3}, ny={:.3}) outside (0,1); element offscreen",
                            nx, ny
                        )),
                        ..Default::default()
                    }));
                }
            }
        };

        self.runner
            .tap_at_norm_coord(nx, ny)
            .await
            .map_err(transport_to_failure)?;
        Ok(())
    }

    /// Tap a selector via an explicit dispatch mode. Used to opt into
    /// the runner-side `daemonProxySynthesize` path for RN Pressable
    /// buttons whose `RCTTouchHandler` gesture recognizer does not fire
    /// `onPress` when the host-side `tap_at_norm_coord` Apple native
    /// event chain is used.
    ///
    /// `TapMode::DaemonProxySynthesize` routes through `POST /tap`
    /// (runner resolves selector + synthesizes via
    /// `XCTRunnerDaemonSession.daemonProxy
    /// ._XCT_synthesizeEvent:completion:`). The existing
    /// [`SimctlDriver::tap`] method keeps the host-resolve +
    /// `tap_at_norm_coord` default for callers that don't need the
    /// daemonProxy alternative.
    pub async fn tap_with_mode(
        &self,
        selector: &Selector,
        mode: TapMode,
        include: Option<IncludeScope>,
    ) -> Result<(), ExpectationFailure> {
        let start = Instant::now();
        let timeout = Duration::from_millis(TOTAL_TIMEOUT_MS);

        loop {
            match self.runner.tap(selector, mode, include).await {
                Ok(_result) => return Ok(()),
                Err(e) => {
                    if start.elapsed() > timeout {
                        return Err(transport_to_failure(e));
                    }
                    sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
                    continue;
                }
            }
        }
    }

    /// Double-tap a selector via swift sim-side XCUIElement.doubleTap().
    /// 5s implicit-wait + retry on transport (mirrors
    /// [`Self::tap_with_mode`]). Same as Maestro `doubleTapOn`.
    pub async fn double_tap(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<(), ExpectationFailure> {
        let start = Instant::now();
        let timeout = Duration::from_millis(TOTAL_TIMEOUT_MS);
        loop {
            match self.runner.double_tap(selector, include).await {
                Ok(_result) => return Ok(()),
                Err(e) => {
                    if start.elapsed() > timeout {
                        return Err(transport_to_failure(e));
                    }
                    sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
                    continue;
                }
            }
        }
    }

    /// Long-press a selector for `duration` via swift sim-side
    /// XCUIElement.press(forDuration:). 5s implicit-wait + retry on
    /// transport. Same as Maestro `longPressOn`.
    pub async fn long_press(
        &self,
        selector: &Selector,
        duration: Duration,
        include: Option<IncludeScope>,
    ) -> Result<(), ExpectationFailure> {
        let start = Instant::now();
        let timeout = Duration::from_millis(TOTAL_TIMEOUT_MS);
        let duration_ms = duration.as_millis().min(u64::MAX as u128) as u64;
        loop {
            match self.runner.long_press(selector, duration_ms, include).await {
                Ok(_result) => return Ok(()),
                Err(e) => {
                    if start.elapsed() > timeout {
                        return Err(transport_to_failure(e));
                    }
                    sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
                    continue;
                }
            }
        }
    }

    /// Rotate sim via swift `XCUIDevice.shared.orientation`. Routes to
    /// the runner-client `set_orientation` → POST /set-orientation.
    pub async fn set_orientation(
        &self,
        orientation: Orientation,
    ) -> Result<(), ExpectationFailure> {
        self.runner
            .set_orientation(orientation.as_wire())
            .await
            .map_err(transport_to_failure)?;
        Ok(())
    }

    /// Fill text into the focused / matched input.
    ///
    /// **chunked-fill**: the swift runner's daemon `_XCT_sendString`
    /// path bursts every keystroke at up to 200 chars/sec, which on
    /// real-sim outraces React Native's `onChangeText` debounce on
    /// the main thread — only the leading 2-3 keystrokes commit
    /// before later ones are dropped by the JS thread reconciler.
    /// The fix is host-side: split `text` into 1-char chunks and post
    /// each to runner `/fill` separately, with an `INTER_CHAR_PAUSE_MS`
    /// gap so the JS thread can flush its onChangeText callback before
    /// the next keystroke fires.
    ///
    /// **selector-type dispatch** (mirrors `find()`): runner `/fill`
    /// only accepts text selectors (`selector.text` field or the
    /// `_focused_` magic). For Id / Label / Role / Anchor /
    /// Text-with-modifiers selectors, host-resolve + tap the target
    /// first to give it keyboard focus, then fill via the `_focused_`
    /// magic. Text-without-modifiers selectors take the fast path
    /// (direct chunked fill).
    pub async fn fill(
        &self,
        selector: &Selector,
        text: &str,
        include: Option<IncludeScope>,
    ) -> Result<(), ExpectationFailure> {
        if can_use_find_route(selector) {
            self.chunked_fill_runner(selector, text, include).await
        } else if matches!(selector, Selector::Focused { .. }) {
            // When the caller passes a Focused selector (e.g. via
            // `Step::InputText` → `App::fill(&focused(), text)`), skip
            // the pre-tap: `Focused` doesn't need a specific element,
            // it's routing intent = "type into whatever is active,
            // via key-event dispatch". The runner's `/fill` handler
            // routes `_focused_` selector to daemon-level key event
            // sending regardless of a11y-focus state — exactly the
            // RN hidden-input case.
            self.chunked_fill_runner(selector, text, include).await
        } else {
            self.tap(selector, include).await?;
            sleep(Duration::from_millis(300)).await;
            let focused = Selector::Focused {
                focused: True(true),
            };
            self.chunked_fill_runner(&focused, text, include).await
        }
    }

    async fn chunked_fill_runner(
        &self,
        selector: &Selector,
        text: &str,
        include: Option<IncludeScope>,
    ) -> Result<(), ExpectationFailure> {
        const INTER_CHAR_PAUSE_MS: u64 = 50;
        let chars: Vec<char> = text.chars().collect();
        if chars.len() <= 1 {
            return self
                .runner
                .fill(selector, text, include)
                .await
                .map_err(transport_to_failure)
                .map(|_| ());
        }
        for (i, ch) in chars.iter().enumerate() {
            let chunk = ch.to_string();
            self.runner
                .fill(selector, &chunk, include)
                .await
                .map_err(transport_to_failure)?;
            if i + 1 < chars.len() {
                sleep(Duration::from_millis(INTER_CHAR_PAUSE_MS)).await;
            }
        }
        Ok(())
    }

    /// Clear the focused / matched input.
    ///
    /// Selector-type dispatch (mirrors `fill()`).
    ///
    /// The runner `/clear` route only accepts text selectors (`selector.text`
    /// field) or the `_focused_` magic. For Id / Label / Role / Anchor /
    /// Text-with-modifiers selectors, host-resolve + tap the target first
    /// so it owns keyboard focus, then clear via the `_focused_` magic
    /// (single round-trip — clear is not chunked like fill).
    pub async fn clear(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<(), ExpectationFailure> {
        if can_use_find_route(selector) {
            self.runner
                .clear(selector, include)
                .await
                .map_err(transport_to_failure)?;
        } else {
            self.tap(selector, include).await?;
            sleep(Duration::from_millis(300)).await;
            let focused = Selector::Focused {
                focused: True(true),
            };
            self.runner
                .clear(&focused, include)
                .await
                .map_err(transport_to_failure)?;
        }
        Ok(())
    }

    /// `POST /press-key` passthru.
    pub async fn press_key(&self, key: KeyName) -> Result<(), ExpectationFailure> {
        self.runner
            .press_key(key)
            .await
            .map_err(transport_to_failure)?;
        Ok(())
    }

    /// Host-side scroll-until-visible loop. Alternates
    /// `driver.tree + resolve_selector` (host-side probe) with
    /// `runner.swipe_once` (single-swipe runner gesture). Up to 30
    /// swipes or 20s timeout.
    pub async fn scroll(
        &self,
        selector: &Selector,
        direction: SwipeDirection,
    ) -> Result<(), ExpectationFailure> {
        let start = Instant::now();
        let timeout = Duration::from_secs(20);
        // Build the resolver cache once outside the swipe loop so regex
        // compile cost is paid once, not per iteration. None case =
        // regex compile error → element-not-found fail-fast
        // (semantically equivalent to silent-None + 30-swipe timeout,
        // but immediate).
        let Some(ctx) = ResolverContext::new(selector) else {
            return Err(ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::ElementNotFound),
                message: format!(
                    "scroll({}, '{}'): selector pattern failed to compile",
                    describe_selector(selector),
                    direction
                ),
                selector: Some(selector.clone()),
                hint: Some(
                    "regex Pattern compile error — check selector syntax (unbalanced bracket / invalid escape / etc.)"
                        .into(),
                ),
                ..Default::default()
            }));
        };
        for i in 0..=SCROLL_MAX_SWIPES {
            // Transport retry on tree fetch (see tap above).
            let tree = self.tree_with_retry(None).await?;
            if let Some(node) = resolve_selector_compiled(&tree, selector, &ctx) {
                // v1.0.27 — live on-screen confirmation. Without it a
                // below-the-fold element with a drifted snapshot frame
                // satisfies the probe on swipe 0 and scrollUntilVisible
                // returns WITHOUT scrolling (insight round-5 Ask 13).
                // A refuted confirm means "exists but not on screen
                // yet" — exactly the state another swipe should fix.
                let matched = [node];
                if self.confirm_on_screen(&matched, None).await {
                    return Ok(());
                }
            }
            if i == SCROLL_MAX_SWIPES || start.elapsed() > timeout {
                let visible = collect_visible_summaries(&tree, 10);
                let target = base_text_or_id(selector);
                let suggestions = smix_error::build_suggestions(target.as_deref(), &visible);
                return Err(ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::ElementNotFound),
                    message: format!(
                        "scroll({}, '{}'): element not visible after {} swipes",
                        describe_selector(selector),
                        direction,
                        SCROLL_MAX_SWIPES
                    ),
                    selector: Some(selector.clone()),
                    visible_elements: visible,
                    suggestions,
                    ..Default::default()
                }));
            }
            self.runner
                .swipe_once(direction)
                .await
                .map_err(transport_to_failure)?;
        }
        Ok(())
    }

    /// `POST /swipe-once` passthru.
    pub async fn swipe_once(&self, direction: SwipeDirection) -> Result<(), ExpectationFailure> {
        self.runner
            .swipe_once(direction)
            .await
            .map_err(transport_to_failure)?;
        Ok(())
    }

    /// `POST /hide-keyboard` passthru.
    pub async fn hide_keyboard(&self) -> Result<(), ExpectationFailure> {
        self.runner
            .hide_keyboard()
            .await
            .map_err(transport_to_failure)?;
        Ok(())
    }

    /// `POST /back` passthru.
    pub async fn back(&self) -> Result<(), ExpectationFailure> {
        self.runner.back().await.map_err(transport_to_failure)?;
        Ok(())
    }

    /// Tap at normalized (nx, ny) coordinates — escape hatch for
    /// coord-based maestro yaml port (§9 #3 lift).
    ///
    /// (nx, ny) must be in [0, 1] (normalized to viewport). The runner
    /// converts to device pixels via the Apple native event chain
    /// (same path as the regular `tap()` centroid pipeline).
    ///
    /// **Escape hatch only — selector path (`tap(&selector)`) is the
    /// canonical surface.** This bypasses a11y resolve entirely. See
    /// CLAUDE.md §9 #3.
    pub async fn tap_at_norm_coord(&self, nx: f64, ny: f64) -> Result<(), ExpectationFailure> {
        self.runner
            .tap_at_norm_coord(nx, ny)
            .await
            .map_err(transport_to_failure)?;
        Ok(())
    }

    /// `POST /tap-by-id {id}` passthru — `XCUIElement.tap()` via the
    /// XCTest gesture-recognizer chain. SwiftUI `.sheet` / `.alert` /
    /// `.confirmationDialog` / `.fullScreenCover` dismiss buttons need
    /// this path because the default host-HID-at-coord injects an
    /// IOKit-level touch that doesn't fire the modal's SwiftUI binding
    /// closure. Returns `ElementNotFound` when the runner reports
    /// `ok=false`.
    pub async fn tap_by_id(&self, id: &str) -> Result<(), ExpectationFailure> {
        let ok = self
            .runner
            .tap_by_id(id)
            .await
            .map_err(transport_to_failure)?;
        if !ok {
            return Err(ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::ElementNotFound),
                message: format!("tap_by_id: element not found — id=\"{id}\""),
                hint: Some(
                    "runner XCUIQuery returned no match; check id spelling or wait for screen to settle"
                        .into(),
                ),
                ..Default::default()
            }));
        }
        Ok(())
    }

    /// Eval JS via the app-side WKWebView bridge. Direct HTTP POST to
    /// `127.0.0.1:28080/eval` (iOS sim shares host loopback) — does
    /// NOT use the XCUITest runner. Returns the parsed JS result as a
    /// JSON Value; surfaces bridge error / transport failure as
    /// DriverError.
    pub async fn webview_eval(&self, js: &str) -> Result<serde_json::Value, ExpectationFailure> {
        self.runner.webview_eval(js).await.map_err(|e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: format!("webview_eval: {e}"),
                ..Default::default()
            })
        })
    }

    /// `POST /find-text-by-ocr` passthru — Apple Vision OCR over the
    /// current XCUIScreen screenshot. Returns the matching text
    /// observation's bounding box (UIKit normalized) or `None`.
    pub async fn find_text_by_ocr(
        &self,
        text: &str,
        locales: &[String],
        recognition_level: &str,
    ) -> Result<Option<OcrFrame>, ExpectationFailure> {
        self.runner
            .find_text_by_ocr(text, locales, recognition_level)
            .await
            .map_err(transport_to_failure)
    }

    /// `POST /swipe-at-norm-coord {from, to}` passthru — escape hatch
    /// from-to swipe gesture sibling to [`Self::tap_at_norm_coord`].
    /// `from` / `to` are normalized to viewport `(0, 1)`. §9 #3 lift
    /// companion to `tap_at_coord`.
    pub async fn swipe_at_norm_coord(
        &self,
        from: (f64, f64),
        to: (f64, f64),
    ) -> Result<(), ExpectationFailure> {
        self.runner
            .swipe_at_norm_coord(from, to)
            .await
            .map_err(transport_to_failure)?;
        Ok(())
    }

    /// `POST /foreground {bundleId}` passthru.
    pub async fn foreground(&self, bundle_id: &str) -> Result<(), ExpectationFailure> {
        self.runner
            .foreground(bundle_id)
            .await
            .map_err(transport_to_failure)?;
        Ok(())
    }

    // ---- wait ----------------------------------------------------------

    /// Poll until the selector resolves to a node, or timeout fires.
    /// Returns the matched node (cloned). 5s default budget, 250 ms
    /// poll interval.
    pub async fn wait_for(
        &self,
        selector: &Selector,
        timeout: Duration,
        include: Option<IncludeScope>,
    ) -> Result<A11yNode, ExpectationFailure> {
        let start = Instant::now();
        // Transient tree() transport errors (e.g. sim still launching →
        // /tree returns 500 snapshot_unavailable) are treated the same
        // as selector-not-found — retry within the timeout budget,
        // surface only the last error on timeout. This matches the
        // semantic intent of wait_for ("wait until ready or timeout")
        // and is required for callers that ride out sim boot transients.
        //
        // Build the resolver cache once outside the poll loop. None
        // case = regex compile error → Timeout fail with an explicit
        // compile-error hint (semantically equivalent to silent-None
        // + budget exhaustion, but immediate and actionable).
        let Some(ctx) = ResolverContext::new(selector) else {
            return Err(ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::Timeout),
                message: format!(
                    "waitFor({}): selector pattern failed to compile",
                    describe_selector(selector)
                ),
                selector: Some(selector.clone()),
                hint: Some(
                    "regex Pattern compile error — check selector syntax (unbalanced bracket / invalid escape / etc.)"
                        .into(),
                ),
                ..Default::default()
            }));
        };
        let mut last_transport_err: Option<ExpectationFailure> = None;
        // v1.0.27 — tracks "the tree matched but the live on-screen
        // check refuted it" across poll iterations so the timeout
        // failure can say WHY the wait never greened (below-the-fold
        // element with a drifted snapshot frame — insight round-5
        // Ask 13's wait-pass → tap-miss pair).
        let mut tree_hit_offscreen = false;
        loop {
            match self.tree(include).await {
                Ok(tree) => {
                    if let Some(node) = resolve_selector_compiled(&tree, selector, &ctx) {
                        // v1.0.27 — live on-screen confirmation.
                        // Snapshot frames drift under iOS 26.5 + RN
                        // Fabric; the resolver's frame∩viewport filter
                        // can pass an element that is actually below
                        // the fold. One live probe per tree hit keeps
                        // wait_for / scrollUntilVisible / tapOn in
                        // agreement on "visible".
                        let matched = [node];
                        if self.confirm_on_screen(&matched, include).await {
                            return Ok(node.clone());
                        }
                        tree_hit_offscreen = true;
                    }
                    if start.elapsed() >= timeout {
                        let visible = collect_visible_summaries(&tree, 10);
                        let target = base_text_or_id(selector);
                        let suggestions =
                            smix_error::build_suggestions(target.as_deref(), &visible);
                        let hint = if tree_hit_offscreen {
                            Some(
                                "the a11y tree matched this selector but the LIVE \
                                 on-screen check refuted it every time — the element \
                                 exists with a stale/drifted snapshot frame (typically \
                                 below the fold on iOS 26.5 + RN Fabric). Use \
                                 scrollUntilVisible to bring it into the viewport \
                                 first, or an ocrText tier to assert by pixels."
                                    .to_string(),
                            )
                        } else {
                            None
                        };
                        return Err(ExpectationFailure::new(FailureInit {
                            code: Some(FailureCode::Timeout),
                            message: format!(
                                "waitFor({}) timed out after {:?}",
                                describe_selector(selector),
                                timeout
                            ),
                            selector: Some(selector.clone()),
                            visible_elements: visible,
                            suggestions,
                            hint,
                            ..Default::default()
                        }));
                    }
                    last_transport_err = None;
                }
                Err(e) => {
                    if start.elapsed() >= timeout {
                        return Err(last_transport_err.unwrap_or(e));
                    }
                    last_transport_err = Some(e);
                }
            }
            sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
        }
    }

    // ---- lifecycle -----------------------------------------------------

    /// Idempotent close hook. The current implementation has no
    /// sidecar / no cache to tear down; reserved for future use.
    pub async fn dispose(&self) -> Result<(), ExpectationFailure> {
        Ok(())
    }
}

fn transport_to_failure(e: RunnerTransportError) -> ExpectationFailure {
    let (code, hint) = match &e {
        RunnerTransportError::Unreachable { .. } => (
            FailureCode::DriverError,
            Some("start the runner first: bash scripts/smix-runner-health.sh".to_string()),
        ),
        // The runner can't snapshot the target app, either because
        // it's not foreground or because XCUITest bound to a different
        // bundle. Emit an AI-readable hint that names the resolution
        // channel.
        RunnerTransportError::AppUnavailable { target, reason, .. } => (
            FailureCode::DriverError,
            Some(format!(
                "runner reports snapshot_unavailable — target={} reason={}. \
                 Fix by (a) `smix run --bundle-id <BUNDLE>` so the client sends \
                 App-Bundle-Id header, or (b) `smix run --activate` so the runner \
                 auto-activates the target before snapshot, or (c) foreground the \
                 target app before invocation.",
                target.as_deref().unwrap_or("<unknown>"),
                reason.as_deref().unwrap_or("<no reason>"),
            )),
        ),
        _ => (FailureCode::DriverError, None),
    };
    ExpectationFailure::new(FailureInit {
        code: Some(code),
        message: format!("{e}"),
        hint,
        ..Default::default()
    })
}

/// Extract the base text or id pattern from a Selector for suggestion
/// generation. Returns the literal string of the base form's pattern, or
/// None for selectors whose base doesn't have a literal target (anchor,
/// role-only, focused).
fn base_text_or_id(selector: &Selector) -> Option<String> {
    match selector {
        Selector::Text { text, .. } => match text {
            Pattern::Text(s) => Some(s.clone()),
            Pattern::Regex { regex, .. } => Some(regex.clone()),
        },
        Selector::Id { id, .. } => Some(id.clone()),
        Selector::Label { label, .. } => Some(label.clone()),
        Selector::Role { name, .. } => name.as_ref().map(|p| match p {
            Pattern::Text(s) => s.clone(),
            Pattern::Regex { regex, .. } => regex.clone(),
        }),
        Selector::Focused { .. } | Selector::Anchor { .. } => None,
        // LocalizedText should be desugared to Selector::Text at the
        // adapter layer before reaching the driver. Return None here
        // as the suggestion-hint fallback.
        Selector::LocalizedText { localized_text, .. } => {
            // Best-effort: return the "en" entry as a hint for AI-readable
            // suggestion output if available; else first table entry.
            localized_text
                .get("en")
                .or_else(|| localized_text.values().next())
                .cloned()
        }
        // The OcrText adapter dispatches OCR + tap_at_coord directly
        // and does not go through the driver host-resolve path.
        // base_text_or_id still returns ocr_text as an AI-readable
        // suggestion hint (e.g. for fallback-chain error reports).
        Selector::OcrText { ocr_text, .. } => Some(ocr_text.clone()),
        // AnchorRelative is an escape hatch family (§9 #3); the
        // adapter dispatches directly. Recurse into the anchor
        // sub-selector for the hint.
        Selector::AnchorRelative { anchor, .. } => base_text_or_id(anchor),
        // Point has no element text; report None (the suggestion hint
        // will look elsewhere).
        Selector::Point { .. } => None,
        Selector::Fallback { fallback } => fallback.first().and_then(base_text_or_id),
    }
}

// Only Text selectors with no spatial / index modifiers ride the runner
// `/find` route. Everything else (Id / Label / Role / Focused / Anchor,
// or Text augmented with below / near / nth / first / last / ancestor /
// ...) falls back to host-resolve in `find()` above. The ancestor
// modifier is included here — Apple element query has no parent-chain
// semantic, so ancestor-bearing selectors must host-resolve.
fn can_use_find_route(selector: &Selector) -> bool {
    let Selector::Text { text, modifiers } = selector else {
        return false;
    };
    // v1.0.27 — regex patterns serialize as `{"regex": …, "flags": …}`
    // objects, which the runner /find route's decode (expecting
    // `selector.text` as a plain string) rejects with 400. Pre-v1.0.27
    // a regex Text selector dispatched here would burn the full
    // transport-retry budget (~8 s) and surface a DriverError instead
    // of evaluating. Only literal patterns ride the live route; regex
    // falls back to host-resolve like every other complex shape.
    if !matches!(text, Pattern::Text(_)) {
        return false;
    }
    modifiers.near.is_none()
        && modifiers.below.is_none()
        && modifiers.above.is_none()
        && modifiers.left_of.is_none()
        && modifiers.right_of.is_none()
        && modifiers.inside.is_none()
        && modifiers.ancestor.is_none()
        && modifiers.nth.is_none()
        && modifiers.first.is_none()
        && modifiers.last.is_none()
}

// Silence unused-import warning for symbols re-exported as future hooks.
// `Modifiers` is used by external callers constructing selectors via
// the smix-selector re-export; we surface it here for SDK convenience.
#[doc(hidden)]
pub use smix_selector::Modifiers as _ModifiersReexport;

// match_text_compiled is exported through smix-selector; surface here
// for downstream test / sdk convenience.
#[doc(hidden)]
pub use smix_selector::match_text_compiled as _match_text_compiled_reexport;
#[allow(dead_code)]
fn _silence_unused_imports() {
    // Touch symbols so unused-import warnings don't trip.
    let _: fn(&A11yNode, &smix_selector::CompiledPattern) -> bool = match_text_compiled;
    let _: Modifiers = Modifiers::default();
    let _: ScreenDescription = ScreenDescription::default();
    let _ = summarize_node;
}

// ===========================================================================
// Cross-platform Driver trait + Android-ready architecture
// ===========================================================================

mod android;
mod ios;
mod traits;

pub use android::AndroidDriver;
pub use traits::{Driver, Platform};

/// Back-compat alias. `SimctlDriver` was renamed to `IosDriver` for
/// cross-platform naming; this alias keeps existing imports
/// `use smix_driver::SimctlDriver` compiling.
pub type SimctlDriver = IosDriver;
