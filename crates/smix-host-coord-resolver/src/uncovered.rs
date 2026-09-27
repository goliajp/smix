//! Where to touch an element that something else is drawn over.
//!
//! An app drawn edge to edge — every app on Android 15 and later — lays
//! its content under the status bar and the navigation bar. A field whose
//! top half sits under the status bar has its centre there too, and a
//! touch at the centre goes to the status bar: the field never takes the
//! focus. The same holds for a control half under the keyboard. The tree
//! says where the element is; it is the windows above it that say where
//! a touch reaches it.

use smix_screen::{A11yNode, Rect, WindowInfo, is_keyboard};

use crate::HostResolveError;

/// What is drawn over part of an element and takes the touches there.
#[derive(Clone, Debug, PartialEq)]
pub struct Cover<'a> {
    /// The part of the screen it covers.
    pub rect: Rect,
    /// What it is.
    pub what: Covering<'a>,
}

/// The kinds of thing that cover an element.
#[derive(Clone, Debug, PartialEq)]
pub enum Covering<'a> {
    /// A window higher in the stack than the element's own.
    Window(&'a WindowInfo),
    /// The software keyboard, where the tree has it as an element (iOS).
    Keyboard,
    /// The status bar, where the tree has it as an element (iOS).
    StatusBar,
}

/// Where a touch reaches an element.
#[derive(Clone, Debug, PartialEq)]
pub enum Touchable<'a> {
    /// At this point, normalized to the app frame.
    At(f64, f64),
    /// Nowhere: every part of it that shows is under these.
    Covered(Vec<Cover<'a>>),
}

/// Where to touch `node`, one of `tree`'s nodes.
///
/// Its centre, when that shows and nothing is over it. Otherwise the
/// centre of the largest part of it that shows and that nothing is over,
/// and [`Touchable::Covered`] when there is no such part.
///
/// # Errors
///
/// As [`crate::resolve_to_norm_coord`]: an app frame or a node frame with
/// no area, and a centre outside the app frame.
pub fn touch_point<'a>(
    tree: &'a A11yNode,
    node: &A11yNode,
) -> Result<Touchable<'a>, HostResolveError> {
    let frame = tree.bounds;
    if frame.w <= 0.0 || frame.h <= 0.0 {
        return Err(HostResolveError::UnknownAppFrame);
    }
    let b = node.bounds;
    if b.w <= 0.0 || b.h <= 0.0 {
        return Err(HostResolveError::EmptyMatchedFrame);
    }
    let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
    let (nx, ny) = (cx / frame.w, cy / frame.h);
    if nx <= 0.0 || nx >= 1.0 || ny <= 0.0 || ny >= 1.0 {
        return Err(HostResolveError::CentroidOutOfFrame { nx, ny });
    }
    let Some(shows) = intersect(node.visible_bounds.unwrap_or(b), frame) else {
        return Err(HostResolveError::EmptyMatchedFrame);
    };
    let covers = covers_of(tree, node, shows);
    if contains(shows, cx, cy) && !covers.iter().any(|c| contains(c.rect, cx, cy)) {
        return Ok(Touchable::At(nx, ny));
    }
    let mut left = vec![shows];
    for c in &covers {
        left = left.into_iter().flat_map(|p| subtract(p, c.rect)).collect();
    }
    let Some(best) = left
        .into_iter()
        .max_by(|a, b| (a.w * a.h).total_cmp(&(b.w * b.h)))
    else {
        return Ok(Touchable::Covered(covers));
    };
    Ok(Touchable::At(
        (best.x + best.w / 2.0) / frame.w,
        (best.y + best.h / 2.0) / frame.h,
    ))
}

/// Everything over `shows` that takes the touches there.
///
/// A window higher in the stack than the one holding `node`, where it
/// takes touches, when both say where they stand; and the keyboard or the status bar where the
/// tree carries them as elements rather than as windows, unless `node`
/// is inside them.
fn covers_of<'a>(tree: &'a A11yNode, node: &A11yNode, shows: Rect) -> Vec<Cover<'a>> {
    let mut covers = Vec::new();
    let own = tree
        .children
        .iter()
        .find(|w| w.window.is_some() && holds(w, node))
        .and_then(|w| w.window.as_ref())
        .and_then(|i| i.layer);
    if let Some(own) = own {
        for w in &tree.children {
            // Where it takes touches, not its bounds: the keyboard's window
            // spans the screen and takes touches only on the keys.
            if let Some(info) = &w.window
                && info.layer.is_some_and(|l| l > own)
                && let Some(rect) = info.touchable.and_then(|t| intersect(t, shows))
            {
                covers.push(Cover {
                    rect,
                    what: Covering::Window(info),
                });
            }
        }
    }
    let mut stack = vec![tree];
    while let Some(n) = stack.pop() {
        let what = if is_keyboard(n) {
            Some(Covering::Keyboard)
        } else if n.raw_type == "statusBar" {
            Some(Covering::StatusBar)
        } else {
            None
        };
        match what {
            // A window is judged by its layer above, not by what it is.
            Some(what) if n.window.is_none() && !holds(n, node) => {
                if let Some(rect) = intersect(n.bounds, shows) {
                    covers.push(Cover { rect, what });
                }
            }
            _ => stack.extend(n.children.iter()),
        }
    }
    covers
}

/// Whether `node` is `n` or under it — by identity: two nodes can share
/// every field.
fn holds(n: &A11yNode, node: &A11yNode) -> bool {
    std::ptr::eq(n, node) || n.children.iter().any(|c| holds(c, node))
}

fn contains(r: Rect, x: f64, y: f64) -> bool {
    x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h
}

fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let w = (a.x + a.w).min(b.x + b.w) - x;
    let h = (a.y + a.h).min(b.y + b.h) - y;
    (w > 0.0 && h > 0.0).then_some(Rect { x, y, w, h })
}

/// `r` less `c`: up to four rectangles, the bands above and below `c`
/// across the whole of `r`, and those left and right of it in between.
fn subtract(r: Rect, c: Rect) -> Vec<Rect> {
    let Some(i) = intersect(r, c) else {
        return vec![r];
    };
    let bands = [
        Rect {
            x: r.x,
            y: r.y,
            w: r.w,
            h: i.y - r.y,
        },
        Rect {
            x: r.x,
            y: i.y + i.h,
            w: r.w,
            h: r.y + r.h - (i.y + i.h),
        },
        Rect {
            x: r.x,
            y: i.y,
            w: i.x - r.x,
            h: i.h,
        },
        Rect {
            x: i.x + i.w,
            y: i.y,
            w: r.x + r.w - (i.x + i.w),
            h: i.h,
        },
    ];
    bands
        .into_iter()
        .filter(|b| b.w > 0.0 && b.h > 0.0)
        .collect()
}
