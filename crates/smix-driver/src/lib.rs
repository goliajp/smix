//! smix-driver — the decide layer.
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

/// Which device a runner port actually reaches. Lives with the client
/// because that is the layer that opens the connection.
pub use smix_runner_client::port_owner;
pub use smix_runner_client::{
    HttpRunnerClient, IncludeScope, OcrFrame, OwnerProbe, RunnerScrollSelector,
    RunnerTransportError, SystemPopup, TapMode,
};

use smix_runner_client::TouchVerdict;

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

/// Scripts the Android recogniser can read.
///
/// ML Kit's Latin package is what the Android runner ships, so a needle
/// in Chinese, Japanese, Korean or Cyrillic cannot be read there — and
/// asking for it produced "no matching text", which is a sentence about
/// the screen when the truth is about the recogniser. Invariant 9 #1 ③:
/// say what this device cannot do.
///
/// Pure, and by script rather than by tag, because `zh-Hans` and `zh` and
/// `zh-Hant-HK` are one answer.
#[must_use]
pub fn latin_script_only(locales: &[String]) -> Option<&str> {
    locales.iter().find_map(|l| {
        let tag = l.to_ascii_lowercase();
        [
            "zh", "ja", "ko", "ru", "uk", "bg", "sr", "el", "ar", "he", "hi", "th",
        ]
        .into_iter()
        .find(|p| tag == *p || tag.starts_with(&format!("{p}-")))
        .map(|_| l.as_str())
    })
}

/// The recognition level every OCR caller asks for.
///
/// A literal in two places is a literal that will differ in one of them.
/// The SDK had it; the CLI would have been the second copy.
pub const OCR_RECOGNITION_LEVEL: &str = "accurate";

/// The locales to read with, when the caller named none.
///
/// Empty means "whatever the session is", and the session's answer is
/// English until something sets it. Callers pass their own list through
/// untouched.
#[must_use]
pub fn ocr_locales(given: &[String]) -> Vec<String> {
    if given.is_empty() {
        vec!["en".to_string()]
    } else {
        given.to_vec()
    }
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
            .map(|t| t.root)
            .map_err(transport_to_failure)
    }

    /// Aggregate `ScreenDescription` (DFS visible summaries; no
    /// screenshot here — caller adds when needed).
    pub async fn describe(&self) -> Result<ScreenDescription, ExpectationFailure> {
        let tree = self.tree(None).await?;
        Ok(ScreenDescription {
            screenshot: None,
            elements: collect_visible_summaries(&tree, DEFAULT_VISIBLE_LIMIT),
            front_app: front_app_of(&tree),
            // `summary` stays empty by contract: the field docs say the
            // caller writes it, and there is no single honest source
            // for it here. The other two used to be empty for no
            // reason at all.
            summary: String::new(),
            captured_at: captured_at_unix_millis(),
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
                // The live route asks for on-screen (frame ∩
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
            // Tree-resolve branch gains the same live
            // on-screen confirmation as wait_for. See
            // `confirm_on_screen` for semantics.
            let matched = resolve_selector_all(&tree, selector);
            if matched.is_empty() {
                return Ok(false);
            }
            Ok(self.confirm_on_screen(&matched, include).await)
        }
    }

    /// Live on-screen confirmation for tree-matched nodes.
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
    /// the tree verdict stands; OCR tiers remain the fallback for
    /// handle-less degraded trees.
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
    /// Tap a selector, and report what the touch landed on.
    ///
    /// Returns an outcome rather than unit: "the touch was synthesised"
    /// and "the element was tapped" are different claims, and this used
    /// to make the first while callers read the second.
    /// Stop before a tap that cannot land where the tree says.
    ///
    /// A runner without `/coordinate-space` answers nothing and this
    /// returns `Ok(())`: those runners drive apps correctly, and
    /// failing them for the absence of a check would be the check
    /// causing the outage it exists to prevent.
    async fn refuse_if_spaces_disagree(&self) -> Result<(), ExpectationFailure> {
        // A transport error here is not this check's business — the tap
        // itself is about to hit the same transport and will report it
        // in its own terms.
        let Ok(Some(space)) = self.runner.coordinate_space(0.5, 0.5).await else {
            return Ok(());
        };
        match decide_tap_outcome(&space) {
            TapSpaceVerdict::Proceed => Ok(()),
            TapSpaceVerdict::Refuse { message } => Err(ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::CoordinateSpaceMismatch),
                message,
                ..Default::default()
            })),
        }
    }

    pub async fn tap(
        &self,
        selector: &Selector,
        include: Option<IncludeScope>,
    ) -> Result<ActOutcome, ExpectationFailure> {
        let start = Instant::now();
        let timeout = Duration::from_millis(TOTAL_TIMEOUT_MS);

        // Before resolving anything: if the point this is about to
        // compute will be read in a different space than it is computed
        // in, the tap cannot land where the tree says the element is,
        // and every signal after this line would say it did.
        self.refuse_if_spaces_disagree().await?;

        let (nx, ny, aimed) = loop {
            // Transport retry parity with wait_for / find. Tree fetch
            // transient transport drops (runner socket refusal /
            // concurrent-handling hiccup) are re-tried in-loop.
            let tree = self.tree_with_retry(include).await?;
            match resolve_to_norm_coord(&tree, selector) {
                // The matched node is kept, not just its centre: the
                // runner reports what the tapped point turned out to be
                // inside, and that is only worth anything next to what
                // was aimed at.
                Ok(coord) => {
                    let node = resolve_selector(&tree, selector);
                    // Found, and out of reach. A modal leaves what is behind
                    // it in the tree and swallows touches aimed there, so
                    // this tap would be dispatched, land on nothing, and
                    // report success — which is what it did until now.
                    //
                    // Only an explicit no refuses. A runner that does not
                    // fill the field says nothing, and treating silence as
                    // a refusal would take tapping away from everyone who
                    // has not upgraded.
                    if let Some(n) = node {
                        if let TouchVerdict::Refuse(why) =
                            smix_runner_client::touch_verdict(n.hittable)
                        {
                            return Err(ExpectationFailure::new(FailureInit {
                                code: Some(FailureCode::NotVisible),
                                message: format!(
                                    "{}: {why}",
                                    describe_selector(selector)
                                ),
                                selector: Some(selector.clone()),
                                visible_elements: collect_visible_summaries(&tree, 10),
                                ..Default::default()
                            }));
                        }
                    }
                    let aimed = node.map(|n| HitElement {
                        identifier: n.identifier.clone().unwrap_or_default(),
                        label: n.label.clone().unwrap_or_default(),
                        frame: (n.bounds.x, n.bounds.y, n.bounds.w, n.bounds.h),
                    });
                    break (coord.0, coord.1, aimed);
                }
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

        let landed = self
            .runner
            .tap_at_norm_coord(nx, ny)
            .await
            .map_err(transport_to_failure)?;
        let chain: Vec<HitElement> = landed
            .chain
            .iter()
            .map(|e| HitElement {
                identifier: e.identifier.clone(),
                label: e.label.clone(),
                frame: (e.frame.x, e.frame.y, e.frame.w, e.frame.h),
            })
            .collect();
        let Some(aimed) = aimed else {
            return Ok(ActOutcome {
                target: None,
                observed: chain,
                verdict: ActVerdict::Unconfirmable(
                    "the selector resolved to a coordinate but not to a node, so \
                     there is nothing to compare the tapped point against"
                        .into(),
                ),
            });
        };
        // An empty chain is the one case that is NOT judged. A runner
        // older than the field answers without it, and that is
        // indistinguishable on the wire from a point that landed
        // outside everything — failing both would break every flow
        // driving an older runner.
        let verdict = if chain.is_empty() {
            ActVerdict::Unconfirmable(
                "the runner reported no elements at the tapped point; it may \
                 predate the field that carries them"
                    .into(),
            )
        } else {
            tap_landed_within(&aimed, &chain)
        };
        if let ActVerdict::Missed(why) = &verdict {
            if tap_mismatch_is_fatal() {
                return Err(ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::TapMissed),
                    message: format!("tap did not land where it aimed: {why}"),
                    selector: Some(selector.clone()),
                    hint: Some(
                        "the screen moved between the tree fetch and the tap; \
                         wait for it to settle first. Set \
                         SMIX_TAP_HIT_MISMATCH=warn to downgrade this to a \
                         warning while migrating a suite."
                            .into(),
                    ),
                    ..Default::default()
                }));
            }
            eprintln!("smix: warning: tap did not land where it aimed: {why}");
        }
        Ok(ActOutcome {
            target: Some(aimed),
            observed: chain,
            verdict,
        })
    }

    /// Tap a selector several times in a row.
    ///
    /// Resolves once, then hands the runner a burst: one synthesise
    /// carrying `times` touches spaced by `interval_ms`. The spacing is
    /// the number given, not the round-trip latency it used to be —
    /// ten separate taps cost ten ~400 ms synthesises, and a gesture
    /// gated on a 500 ms window sat right on that boundary.
    ///
    /// No hit verdict: after the first touch the screen is expected to
    /// react, so what the later ones land on is the app's business
    /// rather than evidence about aim.
    pub async fn tap_burst(
        &self,
        selector: &Selector,
        times: u32,
        interval_ms: Option<u32>,
        hold_ms: Option<u32>,
        include: Option<IncludeScope>,
    ) -> Result<(), ExpectationFailure> {
        let tree = self.tree_with_retry(include).await?;
        let (nx, ny) = resolve_to_norm_coord(&tree, selector).map_err(|_| {
            let visible = collect_visible_summaries(&tree, 10);
            let target = base_text_or_id(selector);
            let suggestions = smix_error::build_suggestions(target.as_deref(), &visible);
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::ElementNotFound),
                message: format!("element not found: {}", describe_selector(selector)),
                selector: Some(selector.clone()),
                visible_elements: visible,
                suggestions,
                ..Default::default()
            })
        })?;
        self.runner
            .tap_at_norm_coord_burst(nx, ny, times, interval_ms, hold_ms)
            .await
            .map(|_| ())
            .map_err(transport_to_failure)
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
        require_runner_resolvable_selector(selector, "/tap")?;
        let start = Instant::now();
        let timeout = Duration::from_millis(TOTAL_TIMEOUT_MS);

        loop {
            match self.runner.tap(selector, mode, include).await {
                Ok(_result) => return Ok(()),
                Err(e) => {
                    // 4xx is the runner refusing the request shape — it
                    // will refuse it identically on every retry, so the
                    // 5s budget bought nothing but latency.
                    let permanent = matches!(
                        &e,
                        smix_runner_client::RunnerTransportError::NonSuccessStatus {
                            status, ..
                        } if (400..500).contains(status) && *status != 404
                    );
                    if permanent || start.elapsed() > timeout {
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
        require_runner_resolvable_selector(selector, "/double-tap")?;
        let start = Instant::now();
        let timeout = Duration::from_millis(TOTAL_TIMEOUT_MS);
        loop {
            match self.runner.double_tap(selector, include).await {
                Ok(_result) => return Ok(()),
                Err(e) => {
                    // 4xx is the runner refusing the request shape — it
                    // will refuse it identically on every retry, so the
                    // 5s budget bought nothing but latency.
                    let permanent = matches!(
                        &e,
                        smix_runner_client::RunnerTransportError::NonSuccessStatus {
                            status, ..
                        } if (400..500).contains(status) && *status != 404
                    );
                    if permanent || start.elapsed() > timeout {
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
    ///
    /// Returns when the touch was held, anchored to this host's clock,
    /// so a caller capturing frames alongside can tell whether they
    /// fall inside the press. See [`press_frame_placement`].
    pub async fn long_press(
        &self,
        selector: &Selector,
        duration: Duration,
        include: Option<IncludeScope>,
    ) -> Result<PressTiming, ExpectationFailure> {
        require_runner_resolvable_selector(selector, "/long-press")?;
        let start = Instant::now();
        let timeout = Duration::from_millis(TOTAL_TIMEOUT_MS);
        let duration_ms = duration.as_millis().min(u64::MAX as u128) as u64;
        loop {
            let sent_ms = host_now_ms();
            match self.runner.long_press(selector, duration_ms, include).await {
                Ok(result) => {
                    return Ok(PressTiming {
                        sent_ms,
                        received_ms: host_now_ms(),
                        latest_down_offset_ms: result.latest_down_offset_ms,
                        earliest_up_offset_ms: result.earliest_up_offset_ms,
                        handler_wall_ms: result.handler_wall_ms,
                    });
                }
                Err(e) => {
                    // 4xx is the runner refusing the request shape — it
                    // will refuse it identically on every retry, so the
                    // 5s budget bought nothing but latency.
                    let permanent = matches!(
                        &e,
                        smix_runner_client::RunnerTransportError::NonSuccessStatus {
                            status, ..
                        } if (400..500).contains(status) && *status != 404
                    );
                    if permanent || start.elapsed() > timeout {
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
        clear_first: bool,
    ) -> Result<(), ExpectationFailure> {
        if can_use_find_route(selector) {
            self.chunked_fill_runner(selector, text, include, clear_first)
                .await
        } else if matches!(selector, Selector::Focused { .. }) {
            // When the caller passes a Focused selector (e.g. via
            // `Step::InputText` → `App::fill(&focused(), text)`), skip
            // the pre-tap: `Focused` doesn't need a specific element,
            // it's routing intent = "type into whatever is active,
            // via key-event dispatch". The runner's `/fill` handler
            // routes `_focused_` selector to daemon-level key event
            // sending regardless of a11y-focus state — exactly the
            // RN hidden-input case.
            self.chunked_fill_runner(selector, text, include, clear_first)
                .await
        } else {
            self.tap(selector, include).await?;
            sleep(Duration::from_millis(300)).await;
            let focused = Selector::Focused {
                focused: True(true),
            };
            self.chunked_fill_runner(&focused, text, include, clear_first)
                .await
        }
    }

    /// Post the text one character at a time.
    ///
    /// `clear_first` belongs to the first chunk alone. Passing it on
    /// every chunk would empty the field before each character and
    /// leave the last one standing on its own — the chunking is a
    /// React-Native workaround, and it must not change what the call
    /// means.
    async fn chunked_fill_runner(
        &self,
        selector: &Selector,
        text: &str,
        include: Option<IncludeScope>,
        clear_first: bool,
    ) -> Result<(), ExpectationFailure> {
        const INTER_CHAR_PAUSE_MS: u64 = 50;
        let chars: Vec<char> = text.chars().collect();
        if chars.is_empty() {
            // Nothing to type, but "fill it with nothing" is a request
            // to empty the field, and dropping it would leave the old
            // value in place.
            if !clear_first {
                return Ok(());
            }
            return self
                .runner
                .fill(selector, text, include, true)
                .await
                .map_err(transport_to_failure)
                .map(|_| ());
        }
        if chars.len() == 1 {
            return self
                .runner
                .fill(selector, text, include, clear_first)
                .await
                .map_err(transport_to_failure)
                .map(|_| ());
        }
        for (i, ch) in chars.iter().enumerate() {
            let chunk = ch.to_string();
            self.runner
                .fill(selector, &chunk, include, clear_first && i == 0)
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
                // Live on-screen confirmation. Without it a
                // below-the-fold element with a drifted snapshot frame
                // satisfies the probe on swipe 0 and scrollUntilVisible
                // returns WITHOUT scrolling.
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
    /// coord-based maestro yaml ports.
    ///
    /// (nx, ny) must be in [0, 1] (normalized to viewport). The runner
    /// converts to device pixels via the Apple native event chain
    /// (same path as the regular `tap()` centroid pipeline).
    ///
    /// **Escape hatch only — the selector path (`tap(&selector)`) is the
    /// canonical surface.** This bypasses a11y resolve entirely.
    pub async fn double_tap_at_norm_coord(
        &self,
        nx: f64,
        ny: f64,
    ) -> Result<(), ExpectationFailure> {
        self.runner
            .double_tap_at_norm_coord(nx, ny)
            .await
            .map_err(transport_to_failure)?;
        Ok(())
    }

    pub async fn long_press_at_norm_coord(
        &self,
        nx: f64,
        ny: f64,
        duration_ms: u64,
    ) -> Result<(), ExpectationFailure> {
        self.runner
            .long_press_at_norm_coord(nx, ny, duration_ms)
            .await
            .map_err(transport_to_failure)?;
        Ok(())
    }

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
    /// `from` / `to` are normalized to viewport `(0, 1)`. Escape-hatch
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
        // Tracks "the tree matched but the live on-screen
        // check refuted it" across poll iterations so the timeout
        // failure can say WHY the wait never greened (below-the-fold
        // element with a drifted snapshot frame, which produces a
        // wait-pass → tap-miss pair).
        let mut tree_hit_offscreen = false;
        loop {
            match self.tree(include).await {
                Ok(tree) => {
                    if let Some(node) = resolve_selector_compiled(&tree, selector, &ctx) {
                        // Live on-screen confirmation.
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

/// What an act did, as opposed to what it attempted.
///
/// The act surface returned `Result<(), ExpectationFailure>` — success
/// was the absence of an error — so nothing could report where a touch
/// actually went. `POST /tap` had carried a rich result all along and
/// the driver discarded it at three call sites; the default path never
/// asked at all.
///
/// `observed` travels even when `verdict` is `Confirmed`, because the
/// verdict cannot see occlusion and the chain can: a caller looking at
/// a scrim next to their button knows something the verdict does not.
#[derive(Clone, Debug, PartialEq)]
pub struct ActOutcome {
    /// The element the selector resolved to, when it resolved to one.
    pub target: Option<HitElement>,
    /// Named elements containing the point, innermost first.
    pub observed: Vec<HitElement>,
    /// Whether the act landed where it aimed.
    pub verdict: ActVerdict,
}

impl ActOutcome {
    /// An act that reached the device with nothing to say about aim.
    ///
    /// Used by paths that dispatch without resolving a target — a raw
    /// coordinate tap has no element to have missed.
    #[must_use]
    pub fn unjudged() -> Self {
        ActOutcome {
            target: None,
            observed: Vec::new(),
            verdict: ActVerdict::Unconfirmable(
                "this path dispatches without resolving a target element".into(),
            ),
        }
    }
}

/// Does a tap that landed outside its target fail the step?
///
/// Yes, by default: reporting success for a touch that went somewhere
/// else is the thing this check exists to stop. `SMIX_TAP_HIT_MISMATCH
/// =warn` downgrades it, so a suite written before the check can be
/// moved over a flow at a time rather than all at once.
///
/// Deliberately not the other way round. Shipping it as a warning and
/// promising to make it fail later hands the value of the change to the
/// next release, and the release after that inherits a suite that has
/// been green through every miss.
fn tap_mismatch_is_fatal() -> bool {
    !std::env::var("SMIX_TAP_HIT_MISMATCH")
        .map(|v| v.eq_ignore_ascii_case("warn"))
        .unwrap_or(false)
}

/// One element, as either side describes it.
///
/// Three fields and not a whole `A11yNode`: the question is "is this
/// the thing I aimed at", and every extra field is another way two
/// truthful descriptions can differ for reasons that are not the
/// answer.
#[derive(Clone, Debug, PartialEq)]
pub struct HitElement {
    /// Accessibility identifier, empty when the element has none.
    pub identifier: String,
    /// Accessibility label, empty when the element has none.
    pub label: String,
    /// `(x, y, w, h)` in the app's coordinate space.
    pub frame: (f64, f64, f64, f64),
}

// The two spaces and the stamp are wire shapes — the runner reports
// them and both the client and this crate read them, so they live in
// the crate whose job that is rather than here, where the client could
// not reach them.
pub use smix_runner_wire::{CoordinateSpace, Rect};

/// Whether a tap may proceed, given the spaces it is about to cross.
#[derive(Clone, Debug, PartialEq)]
pub enum TapSpaceVerdict {
    Proceed,
    Refuse { message: String },
}

/// Decide before touching anything.
///
/// Refusing is the whole point. A tap into a space the touch will not
/// be read in still resolves the element, still reports the aim inside
/// it, and still changes nothing — three signals that all point at the
/// caller's selector, which is where the consumer who found this spent
/// their afternoon. Invariant §9 #1 ③: a capability that is not
/// available is a loud error, never a quiet degradation.
pub fn decide_tap_outcome(space: &CoordinateSpace) -> TapSpaceVerdict {
    if space.spaces_agree {
        return TapSpaceVerdict::Proceed;
    }

    let app = space.app_frame;
    let root = space.snapshot_root_frame;
    // Say which way round each rectangle is, and what the stamp implies
    // the screen must be. Two identical numbers printed under the word
    // "different" read as agreement; the reader has to be able to see
    // that the frames match each other and the stamp is the odd one.
    let shape = |w: f64, h: f64| if w > h { "landscape" } else { "portrait" };
    let stamped_shape = match space.event_record_orientation.as_str() {
        "landscapeLeft" | "landscapeRight" => "landscape",
        _ => "portrait",
    };
    let message = format!(
        "this device's screen and the touch it would receive are described in \
         different spaces, so a tap aimed from the tree would not land where the \
         tree says it is.\n\
         \x20 the app is laid out    {aw}x{ah} ({app_shape})\n\
         \x20 the tree agrees        {rw}x{rh} ({root_shape})\n\
         \x20 the touch is stamped   {stamp} — so its coordinates are read \
         against a {stamped_shape} screen (the device reports {device})\n\
         \n\
         This is not your selector, and it is not the app: smix would have \
         reported this tap as aimed inside its target and moved nothing. \
         Refusing instead.\n\
         Portrait works. If you need this screen driven now, drive it portrait; \
         there is no coordinate you can pass that works around it, because the \
         point is recomputed after you pass it.",
        aw = app.w,
        ah = app.h,
        rw = root.w,
        rh = root.h,
        app_shape = shape(app.w, app.h),
        root_shape = shape(root.w, root.h),
        stamped_shape = stamped_shape,
        stamp = space.event_record_orientation,
        device = space.device_orientation,
    );
    TapSpaceVerdict::Refuse { message }
}

/// What an act turned out to have done.
#[derive(Clone, Debug, PartialEq)]
pub enum ActVerdict {
    /// The point aimed at was inside the element aimed at, as the
    /// accessibility snapshot describes the screen.
    ///
    /// Not "the touch arrived". The comparison is geometric and happens
    /// entirely in snapshot space; whether the synthesised event is then
    /// read in that same space is a separate question, and on a landscape
    /// screen the answer is no.
    Confirmed,
    /// The point was inside something else, or inside nothing.
    Missed(String),
    /// Nothing comparable came back.
    ///
    /// Its own verdict rather than a pass, because "I could not tell"
    /// and "it landed" are different facts and only one of them is
    /// what `tapOn` claims.
    Unconfirmable(String),
}

/// Tolerance, in points, for comparing frames.
///
/// A frame makes a round trip — the host normalises the centre against
/// the app frame, the runner multiplies it back — so exact equality
/// would fail on arithmetic rather than on aim.
const FRAME_TOLERANCE_PT: f64 = 1.0;

/// Wall clock in milliseconds, for anchoring a press window and the
/// captures taken alongside it to the same timeline.
#[must_use]
pub fn host_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// What the runner reported about when a press was actually held,
/// paired with what the host observed about the round trip.
///
/// Offsets are measured by the runner from the moment its handler was
/// entered, not from any shared clock. Nothing here assumes the
/// simulator and the host agree on the time of day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PressTiming {
    /// Host clock when the request went out.
    pub sent_ms: u64,
    /// Host clock when the response came back.
    pub received_ms: u64,
    /// Runner clock, handler entry → the latest instant the touch could
    /// have gone down.
    ///
    /// The runner cannot see inside `press(forDuration:)`, so it reports
    /// bounds rather than instants: the call spanned `[A, B]` and held
    /// for `d`, so the touch went down no later than `B - d` and lifted
    /// no earlier than `A + d`. Composing these as if they were the
    /// instants themselves would widen the window rather than narrow it,
    /// which is why they are named for the bound they are.
    pub latest_down_offset_ms: u64,
    /// Runner clock, handler entry → the earliest instant the touch
    /// could have lifted.
    pub earliest_up_offset_ms: u64,
    /// Runner clock, handler entry → handler return.
    pub handler_wall_ms: u64,
}

/// When a captured frame's pixels were sampled, as an interval.
///
/// A screenshot is not instantaneous — around 230ms on an M-series
/// simulator — and nothing says which instant inside that the pixels
/// come from. So a capture is an interval, and only a capture whose
/// whole interval sits inside the press is during the press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptureSpan {
    /// Host clock when the capture was started.
    pub start_ms: u64,
    /// Host clock when the capture returned.
    pub end_ms: u64,
}

/// Where a captured frame sits relative to the press.
#[derive(Clone, Debug, PartialEq)]
pub enum FramePlacement {
    /// The touch was provably still down for the whole capture.
    DuringPress,
    /// The touch was provably not down for part of the capture.
    Outside(String),
    /// It cannot be placed either way.
    ///
    /// Its own answer rather than a pass, because the whole point of
    /// the verb is to hand back a frame of a held state, and a frame
    /// that might be of a resting one is what sent the consumer who
    /// asked for this down a wrong path in the first place.
    Uncertain(String),
}

impl PressTiming {
    /// How much of the round trip is unaccounted for by the handler,
    /// and therefore could have been spent in either direction.
    fn transit_ambiguity_ms(&self) -> u64 {
        self.received_ms
            .saturating_sub(self.sent_ms)
            .saturating_sub(self.handler_wall_ms)
    }

    /// The interval over which the touch is certainly down, on the host
    /// clock.
    ///
    /// The handler was entered somewhere in `[sent, sent +
    /// ambiguity]`, so touch-down is no later than `sent + ambiguity +
    /// down_offset` and lift-up is no earlier than `sent + up_offset`.
    /// Those two bounds are the interval that holds under every
    /// division of the round trip.
    fn certainly_held_ms(&self) -> Option<(u64, u64)> {
        let start = self.sent_ms + self.transit_ambiguity_ms() + self.latest_down_offset_ms;
        let end = self.sent_ms + self.earliest_up_offset_ms;
        (start < end).then_some((start, end))
    }
}

impl PressTiming {
    /// A press whose bounds the runner did not report.
    ///
    /// Every offset is zero, so no interval is certainly held and every
    /// frame comes back `Uncertain`. That is the honest answer for a
    /// platform that cannot time its own press.
    #[must_use]
    pub fn unplaceable() -> Self {
        PressTiming {
            sent_ms: 0,
            received_ms: 0,
            latest_down_offset_ms: 0,
            earliest_up_offset_ms: 0,
            handler_wall_ms: 0,
        }
    }
}

/// Was this frame captured while the touch was down?
///
/// Judged against the interval that holds under every division of the
/// round trip between request transit, handler work, and response
/// transit — so a `DuringPress` needs no assumption about which of
/// those the unaccounted milliseconds went to, and no assumption that
/// the simulator's clock agrees with the host's.
#[must_use]
pub fn press_frame_placement(press: &PressTiming, frame: &CaptureSpan) -> FramePlacement {
    let held_ms = press
        .earliest_up_offset_ms
        .saturating_sub(press.latest_down_offset_ms);
    let ambiguity = press.transit_ambiguity_ms();
    let Some((held_from, held_to)) = press.certainly_held_ms() else {
        return FramePlacement::Uncertain(format!(
            "the press was held for {held_ms}ms but {ambiguity}ms of the \
             round trip is unaccounted for, so no instant on this host's \
             clock is certainly inside it — hold for longer than {ambiguity}ms"
        ));
    };
    if frame.start_ms >= held_from && frame.end_ms <= held_to {
        return FramePlacement::DuringPress;
    }
    if frame.start_ms >= held_to {
        return FramePlacement::Outside(format!(
            "the capture started {}ms after the touch could still have been \
             down — the press was {held_ms}ms and a capture takes around \
             230ms, so it has to start earlier or the press has to be longer",
            frame.start_ms - held_to
        ));
    }
    if frame.end_ms <= held_from {
        return FramePlacement::Outside(format!(
            "the capture finished {}ms before the touch was certainly down",
            held_from - frame.end_ms
        ));
    }
    FramePlacement::Uncertain(format!(
        "the capture ran {}..{} and the touch was certainly down only over \
         {held_from}..{held_to}, so its pixels could be from either side of \
         the boundary",
        frame.start_ms, frame.end_ms
    ))
}

/// Did the touch land inside the element it aimed at?
///
/// `chain` is every named element containing the tapped point, as the
/// runner found them after synthesising the touch.
///
/// # Why containment and not identity
///
/// The first version of this asked whether the element at the point
/// *was* the element aimed at. A live tree says why that is wrong. At
/// the centre of the first row of Settings, the named elements
/// containing the point are:
///
/// ```text
/// staticText  "登录以访问iCloud数据…"                      area 7283
/// button      id=com.apple.settings.primaryAppleAccount   area 33423
/// application id=com.apple.Preferences                    area 351348
/// ```
///
/// A flow aiming at that button taps its centre, and the innermost
/// element there is the button's own label. Identity would call a
/// perfectly good tap a miss — and text nested inside a row is what
/// every list screen looks like. Containment gets it right: the button
/// is on the chain.
///
/// # WHAT THIS CANNOT SEE
///
/// **Occlusion.** A scrim covering the aimed element contains the
/// point too, so this passes. The snapshot the runner walks carries no
/// z-order (`TreeRoute.swift`: snapshots are dead frames), and
/// `isHittable` — Apple's own answer — has been rejected here twice
/// on purpose: it reports false for an element that is reachable in
/// the AX tree but visually covered, which is exactly the see-through
/// tap `SmixRunnerUITests.swift` performs deliberately, and it broke a
/// QA-overlay assertion in v1.0.27.
///
/// So this closes the stale-frame half of "the tap reported success and
/// nothing happened" and not the covered-element half. The whole chain
/// travels in the outcome regardless, so a caller can see the scrim
/// even when the verdict passes.
pub fn tap_landed_within(aimed: &HitElement, chain: &[HitElement]) -> ActVerdict {
    if chain.is_empty() {
        return ActVerdict::Missed(format!(
            "aimed at {} and the tapped point held nothing — the element \
             moved between the tree fetch and the tap, or its frame was \
             stale",
            describe_hit(aimed)
        ));
    }
    if chain.iter().any(|c| same_element(aimed, c)) {
        return ActVerdict::Confirmed;
    }
    if aimed.identifier.is_empty() && aimed.label.is_empty() {
        return ActVerdict::Unconfirmable(format!(
            "the element aimed at carries neither an identifier nor a \
             label, so it cannot be looked for among the {} element(s) \
             at the tapped point",
            chain.len()
        ));
    }
    ActVerdict::Missed(format!(
        "aimed at {} but the tapped point is inside {} instead",
        describe_hit(aimed),
        chain
            .iter()
            .map(describe_hit)
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

/// Are these two descriptions the same element?
///
/// By the strongest field both carry: identifier, then label, then
/// geometry.
fn same_element(a: &HitElement, b: &HitElement) -> bool {
    if !a.identifier.is_empty() && !b.identifier.is_empty() {
        return a.identifier == b.identifier;
    }
    if !a.label.is_empty() && !b.label.is_empty() {
        return a.label == b.label;
    }
    if a.identifier.is_empty()
        && a.label.is_empty()
        && b.identifier.is_empty()
        && b.label.is_empty()
    {
        let close = |x: f64, y: f64| (x - y).abs() <= FRAME_TOLERANCE_PT;
        return close(a.frame.0, b.frame.0)
            && close(a.frame.1, b.frame.1)
            && close(a.frame.2, b.frame.2)
            && close(a.frame.3, b.frame.3);
    }
    // One is named and the other is not: they are describable in
    // different vocabularies, which is not evidence of sameness.
    false
}

fn describe_hit(e: &HitElement) -> String {
    if !e.identifier.is_empty() {
        format!("id={}", e.identifier)
    } else if !e.label.is_empty() {
        format!("label={:?}", e.label)
    } else {
        format!(
            "an unnamed element at ({:.0},{:.0} {:.0}x{:.0})",
            e.frame.0, e.frame.1, e.frame.2, e.frame.3
        )
    }
}

/// The /tap, /double-tap and /long-press routes decode ONLY a plain
/// literal `selector.text` — the Swift side has no resolver for id /
/// label / role / regex forms, and silently drops modifiers. Reject
/// those here, before the wire: they used to go out anyway, 400, and
/// then burn the full 5s transport-retry budget before surfacing an
/// unrelated-looking error (or, for a modifier, tap the wrong element).
/// Forms the runner-side routes can resolve on their own.
///
/// Named for what it admits rather than what it refuses: the old name
/// said "plain text" while the set was always narrower than that
/// phrase and is now wider. text / id / label are exactly what the
/// runner's NSPredicate expresses directly.
///
/// Everything else stays host-side deliberately. Regex needs the
/// pattern semantics, roles need the rawType→Role table, and spatial
/// or index modifiers need the tree walk — putting any of them behind
/// this wire would mean one contract with two implementations, one of
/// them inside XCUITest where it cannot be tested the same way.
///
/// Public so a gate can ask this rule directly instead of restating
/// it. A restatement would drift the first time the set widened, which
/// is how the actions guide came to document a pairing this had always
/// refused. Not part of the supported surface — hidden from the docs
/// and free to change with the routes it guards.
#[doc(hidden)]
pub fn require_runner_resolvable_selector(
    selector: &Selector,
    route: &str,
) -> Result<(), ExpectationFailure> {
    let default_modifiers = smix_selector::Modifiers::default();
    let ok = match selector {
        Selector::Text { text, modifiers } => {
            matches!(text, smix_selector::Pattern::Text(_)) && *modifiers == default_modifiers
        }
        Selector::Id { modifiers, .. } | Selector::Label { modifiers, .. } => {
            *modifiers == default_modifiers
        }
        _ => false,
    };
    if ok {
        return Ok(());
    }
    Err(ExpectationFailure::new(FailureInit {
        code: Some(FailureCode::DriverError),
        message: format!(
            "{route} resolves text, id and label selectors runner-side; it \
             does not take regex patterns, roles, or spatial/index modifiers. \
             Those resolve against the full tree, which only the host has — \
             use the default tap (host-side resolve) for them."
        ),
        ..Default::default()
    }))
}

/// What a transport error means to a caller, in one place.
///
/// Exported rather than copied: a second translation of the same errors
/// drifts from this one, and the thing that drifts first is the hint —
/// which is the half a reader acts on.
pub fn transport_to_failure(e: RunnerTransportError) -> ExpectationFailure {
    let (code, hint) = match &e {
        RunnerTransportError::Unreachable { .. } => (
            FailureCode::DriverError,
            Some("start the runner first: bash scripts/smix-runner-health.sh".to_string()),
        ),
        // The runner could not snapshot the target app. When it said
        // why, that answer wins: it read `XCUIApplication.state` and
        // this side is guessing from a category string.
        //
        // The guess used to print regardless — three fixes, offered
        // for every cause. Against `not-running` all three are wrong,
        // and a reader following them re-sends a header, adds
        // `--activate`, and foregrounds an app that is not there,
        // while the runner's own hint ("launch it again") sat one line
        // above.
        RunnerTransportError::AppUnavailable {
            target,
            reason,
            category,
            hint,
            ..
        } => (
            FailureCode::DriverError,
            Some(match (category.as_deref(), hint.as_deref()) {
                (Some(cat), Some(h)) if cat != "unknown" => {
                    format!("the runner says {cat}: {h}")
                }
                _ => format!(
                    "runner reports snapshot_unavailable — target={} reason={}. \
                     Fix by (a) `smix run --bundle-id <BUNDLE>` so the client sends \
                     App-Bundle-Id header, or (b) `smix run --activate` so the runner \
                     auto-activates the target before snapshot, or (c) foreground the \
                     target app before invocation.",
                    target.as_deref().unwrap_or("<unknown>"),
                    reason.as_deref().unwrap_or("<no reason>"),
                ),
            }),
        ),
        // The runner answering "I looked and it is not there" is not the
        // transport failing, and the two want opposite fixes. It reached
        // callers as DRIVER_ERROR carrying the wire body verbatim —
        // `{"ok":false,"error":"not_found","selector":{"text":"_focused_"}}`
        // — which is an internal spelling shown to someone who cannot act
        // on it, under a code that says something is broken. Android has
        // always answered ELEMENT_NOT_FOUND to the same situation.
        //
        // Narrow on purpose: a 404 whose body does NOT say `not_found` is
        // a route this runner does not have, which is a version mismatch
        // and genuinely a driver error.
        RunnerTransportError::NonSuccessStatus { status, body, .. }
            if *status == 404 && body.contains("\"not_found\"") =>
        {
            (FailureCode::ElementNotFound, None)
        }
        // Nothing answered the port. Said plainly, because the shape a
        // reader meets is seven steps in a row each reporting
        // `error sending request for url (http://127.0.0.1:22089/tree)`
        // — which reads as seven problems and is one: the runner is not
        // there any more. A consumer spent a while on that before
        // finding the real cause (an AVD with 500 MB free; the system
        // killed the instrumentation).
        //
        // Narrow on purpose. `is_connect` is the connection being
        // refused or reset; a timeout or a half-read body is a runner
        // that IS there and struggling, and telling that reader to
        // restart it would send them the wrong way.
        RunnerTransportError::FetchFailed { endpoint, source } if source.is_connect() => (
            FailureCode::DriverError,
            Some(format!(
                "nothing is listening for {endpoint} any more. The runner answered \
                 earlier in this run, so it went away mid-flight — on Android the \
                 usual cause is the system killing the instrumentation under memory \
                 pressure (check `adb logcat` for `binderDied`, and the emulator's \
                 RAM). Bring it back with `smix runner up`; every step after this \
                 one will report the same thing until you do."
            )),
        ),
        // The runner named its refusal, so the hint can name the next
        // step — and the two keyboard cases send a reader in opposite
        // directions, which is the whole reason the name exists.
        RunnerTransportError::RefusedNaming { kind, .. } => (
            FailureCode::DriverError,
            Some(match kind.as_str() {
                "keyboard_did_not_close" => "the keyboard was there and every dismiss \
                     strategy ran without closing it — the `saw` above names what was \
                     tried and what still holds focus. Tapping the next control often \
                     works when the field will not give focus up; `hideKeyboard` is \
                     already a no-op when no keyboard is present, so a guard around it \
                     is not what this needs."
                    .to_string(),
                "keyboard_state_unknown" => "the runner raised while looking, so nothing \
                     was established about the keyboard — this is not evidence it is \
                     still up. Retry the step; if it repeats, the runner is the thing \
                     to look at, not the screen."
                    .to_string(),
                other => format!(
                    "the runner refused with `{other}` — the `saw` above is what it \
                     observed"
                ),
            }),
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
        // AnchorRelative is an escape hatch family; the
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
    // Regex patterns serialize as `{"regex": …, "flags": …}`
    // objects, which the runner /find route's decode (expecting
    // `selector.text` as a plain string) rejects with 400. A regex
    // Text selector dispatched here would therefore burn the full
    // transport-retry budget (~8 s) and surface a DriverError instead
    // of evaluating. Only literal patterns ride the live route; regex
    // falls back to host-resolve like every other complex shape.
    if !matches!(text, Pattern::Text(_)) {
        return false;
    }
    // `Modifiers::is_empty`, not a list of fields to exclude, because
    // that shape opts a new modifier IN by saying nothing — and `and`
    // (selector conjunction, v3.0) was opted in that way. The runner's
    // `/find` decodes `selector.text` and discards the rest, so
    // `text:Submit` conjoined with `id:save` would have matched on the
    // text alone and answered `found:true` with the conjunction never
    // evaluated. A wrong answer, delivered calmly. Three emitters held
    // their own copy of the same list and had drifted further.
    modifiers.is_empty()
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

/// The bundle id this description was taken from, read off the a11y
/// tree's root identifier.
///
/// The runner writes the app it resolved into that identifier, so the
/// value is already on the wire; nothing new has to be fetched. An
/// empty identifier becomes `None` rather than `Some("")` — an empty
/// string is "I don't know" wearing the costume of "I know", and this
/// field was previously an unconditional empty string for exactly that
/// reason.
#[must_use]
pub fn front_app_of(tree: &A11yNode) -> Option<String> {
    tree.identifier
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Wall-clock milliseconds at capture. Milliseconds, matching the
/// field's documented unit — a seconds clock here would be wrong by
/// three orders of magnitude and still look plausible.
fn captured_at_unix_millis() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

/// Back-compat alias. `SimctlDriver` was renamed to `IosDriver` for
/// cross-platform naming; this alias keeps existing imports
/// `use smix_driver::SimctlDriver` compiling.
pub type SimctlDriver = IosDriver;

#[cfg(test)]
mod runner_resolvable_tests {
    use super::*;
    use smix_selector::{Modifiers, Pattern, Selector};

    /// The three forms a runner-side route can match directly.
    ///
    /// The guard used to accept only plain text, which is why
    /// `dispatch: daemonProxy` could never address an RN testID — the
    /// one thing that escape hatch exists for. The actions guide has
    /// documented that pairing since it was written.
    #[test]
    fn runner_resolvable_accepts_plain_text() {
        let sel = Selector::Text {
            text: Pattern::Text("Sign In".into()),
            modifiers: Modifiers::default(),
        };
        assert!(require_runner_resolvable_selector(&sel, "/tap").is_ok());
    }

    #[test]
    fn runner_resolvable_accepts_id() {
        let sel = Selector::Id {
            id: "btn-login".into(),
            modifiers: Modifiers::default(),
        };
        assert!(require_runner_resolvable_selector(&sel, "/tap").is_ok());
    }

    #[test]
    fn runner_resolvable_accepts_label() {
        let sel = Selector::Label {
            label: "Sign In".into(),
            modifiers: Modifiers::default(),
        };
        assert!(require_runner_resolvable_selector(&sel, "/tap").is_ok());
    }

    /// Regex needs the host's pattern semantics. Accepting it here
    /// would mean a second implementation inside XCUITest.
    #[test]
    fn runner_resolvable_rejects_regex_text() {
        let sel = Selector::Text {
            text: Pattern::Regex {
                regex: "^Sign".into(),
                flags: "i".into(),
            },
            modifiers: Modifiers::default(),
        };
        assert!(require_runner_resolvable_selector(&sel, "/tap").is_err());
    }

    /// Roles need the rawType→Role table, which lives host-side.
    #[test]
    fn runner_resolvable_rejects_role() {
        let sel = Selector::Role {
            role: smix_selector::Role::Button,
            name: None,
            modifiers: Modifiers::default(),
        };
        assert!(require_runner_resolvable_selector(&sel, "/tap").is_err());
    }

    /// Modifiers need the whole tree walk; an accepted form with a
    /// modifier attached would silently drop the modifier.
    #[test]
    fn runner_resolvable_rejects_index_modifier() {
        let sel = Selector::Id {
            id: "row".into(),
            modifiers: Modifiers {
                nth: Some(2),
                ..Modifiers::default()
            },
        };
        assert!(require_runner_resolvable_selector(&sel, "/tap").is_err());
    }
}

#[cfg(test)]
mod describe_meta_tests {
    use super::*;
    use smix_screen::A11yNode;

    fn node_with_identifier(id: Option<&str>) -> A11yNode {
        A11yNode {
            raw_type: "application".into(),
            element_type_raw: 1,
            role: None,
            identifier: id.map(str::to_string),
            label: None,
            title: None,
            placeholder_value: None,
            value: None,
            text: None,
            bounds: smix_screen::Rect {
                x: 0.0,
                y: 0.0,
                w: 390.0,
                h: 844.0,
            },
            enabled: true,
            selected: false,
            has_focus: false,
            visible: true,
            children: vec![],
        }
    }

    /// The bundle id has been on the wire all along: the runner sets it
    /// as the tree root's identifier so host-side smoke can assert on
    /// it. The ledger recorded this field as having no honest source
    /// outside the runner, which was wrong.
    #[test]
    fn describe_meta_front_app_reads_tree_root_identifier() {
        let tree = node_with_identifier(Some("com.apple.Preferences"));
        assert_eq!(
            front_app_of(&tree).as_deref(),
            Some("com.apple.Preferences")
        );
    }

    /// None, not "". An empty string is "I don't know" wearing the
    /// costume of "I know" — the exact confusion this segment exists to
    /// remove.
    #[test]
    fn describe_meta_front_app_is_none_without_root_identifier() {
        assert_eq!(front_app_of(&node_with_identifier(None)), None);
        assert_eq!(front_app_of(&node_with_identifier(Some(""))), None);
    }

    #[test]
    fn describe_meta_captured_at_is_unix_millis() {
        // 2026-01-01T00:00:00Z in ms. A seconds-based clock would be
        // ~1000x smaller and fail here rather than silently mislabel.
        assert!(captured_at_unix_millis() > 1_767_225_600_000.0);
    }

    /// describe() does not own `summary`; the field docs say the caller
    /// fills it. Pinned so "it's empty" reads as the contract rather
    /// than as the same omission the other two fields had.
    #[test]
    fn describe_meta_summary_is_not_produced_here() {
        assert_eq!(smix_screen::ScreenDescription::default().summary, "");
    }
}

#[cfg(test)]
mod find_route_dispatch_tests {
    use super::*;
    use smix_selector::{Modifiers, Pattern, Selector};

    fn text(t: &str, modifiers: Modifiers) -> Selector {
        Selector::Text {
            text: Pattern::Text(t.into()),
            modifiers,
        }
    }

    #[test]
    fn plain_text_rides_the_find_route() {
        assert!(can_use_find_route(&text("Submit", Modifiers::default())));
    }

    /// The runner's `/find` decodes `selector.text` and discards
    /// everything else, so a conjunction sent there is not merely
    /// unsupported — it is silently dropped, and the reply is a
    /// confident `found:true` for a match on the text alone.
    ///
    /// `and` shipped in this cycle and the dispatch predicate, which
    /// listed the modifiers to exclude, did not mention it. Opting in
    /// by saying nothing is what that shape of list does.
    #[test]
    fn a_conjunction_does_not_ride_the_find_route() {
        let sel = text(
            "Submit",
            Modifiers {
                and: vec![Selector::Id {
                    id: "save-button".into(),
                    modifiers: Modifiers::default(),
                }],
                ..Modifiers::default()
            },
        );
        assert!(
            !can_use_find_route(&sel),
            "a conjoined selector must resolve host-side; /find would \
             evaluate the text and ignore the conjunction"
        );
    }
}
