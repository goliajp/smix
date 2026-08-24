#![doc = include_str!("../README.md")]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

//! smix-host-coord-resolver — pure pipeline from `(A11yNode, Selector)`
//! to normalized `(nx, ny)` tap coordinate, with no I/O.
//!
//! This is the host-side resolve step decoupled from any specific
//! injector (HID / Apple native event chain / runner-side mode=resolve
//! / etc.). Higher-level orchestrators (smix-driver, user code) fetch
//! the tree, call this stone to compute the tap coord, then hand it to
//! their preferred injector.
//!
//! # Stone scope
//!
//! Pure function, no async, no I/O, no awareness of any specific
//! injector implementation. Any iOS-automation project that has an
//! [`A11yNode`] (or equivalent that maps to one) and a structured
//! selector can adopt this crate to get host-side resolve.
//!
//! # Pipeline (mirrors smix-driver's `tap` middle section)
//!
//! 1. `resolve_selector(tree, selector)` — DFS + spatial filter +
//!    visibility check.
//! 2. Sanity-check matched node frame (w*h > 0).
//! 3. Compute centroid in app coords.
//! 4. Normalize by `tree.bounds` (app frame).
//! 5. Reject (nx, ny) outside (0, 1) as offscreen.
//!
//! # Example
//!
//! ```
//! use smix_host_coord_resolver::{resolve_to_norm_coord, HostResolveError};
//! use smix_screen::{A11yNode, Rect};
//! use smix_selector::{Modifiers, Pattern, Selector};
//!
//! fn mk(label: &str, bounds: Rect) -> A11yNode {
//!     A11yNode {
//!         raw_type: "other".into(),
//!         element_type_raw: 1,
//!         role: None, identifier: None,
//!         label: Some(label.into()),
//!         title: None, placeholder_value: None, value: None, text: None,
//!         bounds, enabled: true, selected: false, has_focus: false,
//!         visible: true, children: vec![],
//!     }
//! }
//!
//! let mut root = mk("root", Rect { x: 0.0, y: 0.0, w: 390.0, h: 844.0 });
//! root.children.push(mk("Login", Rect { x: 50.0, y: 100.0, w: 200.0, h: 40.0 }));
//!
//! let sel = Selector::Text {
//!     text: Pattern::text("Login"),
//!     modifiers: Modifiers::default(),
//! };
//! let (nx, ny) = resolve_to_norm_coord(&root, &sel).unwrap();
//! assert!((nx - 150.0 / 390.0).abs() < 1e-9);
//! assert!((ny - 120.0 / 844.0).abs() < 1e-9);
//! ```

#![doc(html_root_url = "https://docs.smix.dev/smix-host-coord-resolver")]

use smix_screen::A11yNode;
use smix_selector::Selector;
use smix_selector_resolver::resolve_selector;
use thiserror::Error;

/// Errors returned by [`resolve_to_norm_coord`].
#[derive(Debug, Error, PartialEq)]
pub enum HostResolveError {
    /// The selector matched no node in the tree (after visibility filter).
    #[error("selector matched no node")]
    NotFound,
    /// Matched node has empty bounds (w*h == 0); element is offscreen
    /// or hidden.
    #[error("matched node has empty / offscreen frame")]
    EmptyMatchedFrame,
    /// Tree bounds (`tree.bounds`) have w/h ≤ 0 — unknown app frame,
    /// cannot normalize.
    #[error("tree bounds w/h ≤ 0 — unknown app frame, cannot normalize")]
    UnknownAppFrame,
    /// Computed `(nx, ny)` is outside the open interval `(0, 1)` —
    /// matched node centroid is offscreen.
    #[error("centroid (nx={nx:.3}, ny={ny:.3}) outside (0, 1) — element offscreen")]
    CentroidOutOfFrame {
        /// Normalized x coordinate (would-be tap target).
        nx: f64,
        /// Normalized y coordinate (would-be tap target).
        ny: f64,
    },
}

/// A point at a share of the matched node's box, in app-frame
/// normalized coords.
///
/// [`resolve_to_norm_coord`] answers with the centre, which is what a
/// tap wants. A swipe inside an element wants two points that are not
/// the centre: `(0.5, 0.3)` is halfway across and three tenths down its
/// box. That is the difference between a flow that says "drag this
/// timeline from a third to four fifths" and one that measured 49% of
/// the screen on two devices and took the overlap.
///
/// Same guards as the centre, and for the same reasons: an empty node
/// frame or an unknown app frame is refused rather than divided by, and
/// a point outside the app frame is refused rather than clamped —
/// clamping would swipe somewhere the caller did not ask for and report
/// success.
///
/// `share` is not clamped either. `(1.5, 0.5)` is a caller asking for a
/// point outside the element it named, which is a mistake worth saying
/// out loud rather than quietly turning into the edge.
///
/// # Errors
///
/// [`HostResolveError`], as [`resolve_to_norm_coord`].
#[must_use = "the coordinate should be passed to an injector"]
pub fn resolve_to_norm_point_in_box(
    tree: &A11yNode,
    selector: &Selector,
    share: (f64, f64),
) -> Result<(f64, f64), HostResolveError> {
    let node = resolve_selector(tree, selector).ok_or(HostResolveError::NotFound)?;
    let app_frame = tree.bounds;
    if app_frame.w <= 0.0 || app_frame.h <= 0.0 {
        return Err(HostResolveError::UnknownAppFrame);
    }
    if node.bounds.w <= 0.0 || node.bounds.h <= 0.0 {
        return Err(HostResolveError::EmptyMatchedFrame);
    }
    let x = node.bounds.x + node.bounds.w * share.0;
    let y = node.bounds.y + node.bounds.h * share.1;
    let nx = x / app_frame.w;
    let ny = y / app_frame.h;
    if !(0.0..=1.0).contains(&nx) || !(0.0..=1.0).contains(&ny) {
        return Err(HostResolveError::CentroidOutOfFrame { nx, ny });
    }
    Ok((nx, ny))
}

/// Resolve `selector` against `tree` and produce a normalized tap
/// coordinate `(nx, ny)` in `(0, 1)`.
///
/// `tree.bounds` is treated as the app frame (origin top-left, +y down).
/// The matched node's centroid in app coords is divided by app frame
/// width/height to produce a value in `(0, 1)`.
///
/// Returns `Err(HostResolveError)` for any of: no match, matched node
/// has empty frame, unknown app frame, computed centroid offscreen.
#[must_use = "the coordinate should be passed to an injector"]
pub fn resolve_to_norm_coord(
    tree: &A11yNode,
    selector: &Selector,
) -> Result<(f64, f64), HostResolveError> {
    let node = resolve_selector(tree, selector).ok_or(HostResolveError::NotFound)?;
    let app_frame = tree.bounds;
    if app_frame.w <= 0.0 || app_frame.h <= 0.0 {
        return Err(HostResolveError::UnknownAppFrame);
    }
    if node.bounds.w <= 0.0 || node.bounds.h <= 0.0 {
        return Err(HostResolveError::EmptyMatchedFrame);
    }
    let cx = node.bounds.x + node.bounds.w / 2.0;
    let cy = node.bounds.y + node.bounds.h / 2.0;
    let nx = cx / app_frame.w;
    let ny = cy / app_frame.h;
    if nx <= 0.0 || nx >= 1.0 || ny <= 0.0 || ny >= 1.0 {
        return Err(HostResolveError::CentroidOutOfFrame { nx, ny });
    }
    Ok((nx, ny))
}
