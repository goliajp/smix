//! Where an Android act aims: the point, the box, and the tree read.

use std::time::Duration;

use smix_error::{ExpectationFailure, FailureCode, FailureInit};
use smix_host_coord_resolver::{HostResolveError, resolve_to_norm_coord};
use smix_runner_client::IncludeScope;
use smix_selector::Selector;

use super::AndroidDriver;
use crate::Driver;

/// Host-resolve loop with 5s implicit-wait + 250ms poll.
/// Returns viewport-normalized centroid coord. Shared by tap / double_tap
/// / long_press / fill / clear.
/// The element's own box, viewport-normalized, alongside its centre.
///
/// A fill needs the box, not just the point. A field named by the
/// layout around it — a consumer's views carry the contentDescription
/// on the wrapper and nothing on the input — resolves to the wrapper,
/// and the wrapper's centre can sit on a label rather than on the
/// field. What identifies the field is that it lies *inside* what was
/// named.
pub(super) async fn resolve_rect_with_implicit_wait(
    driver: &AndroidDriver,
    selector: &Selector,
    include: Option<IncludeScope>,
) -> Result<((f64, f64), (f64, f64, f64, f64)), ExpectationFailure> {
    let coord = resolve_with_implicit_wait(driver, selector, include).await?;
    let tree = driver.tree(include).await?;
    let frame = tree.bounds;
    let Some(named) = smix_selector_resolver::resolve_selector(&tree, selector)
        .filter(|_| frame.w > 0.0 && frame.h > 0.0)
    else {
        return Ok((coord, (coord.0, coord.1, 0.0, 0.0)));
    };
    let rect = (
        (named.bounds.x - frame.x) / frame.w,
        (named.bounds.y - frame.y) / frame.h,
        named.bounds.w / frame.w,
        named.bounds.h / frame.h,
    );
    // Aim at the field, not at the middle of what names it. A layout
    // whose contentDescription names the field it wraps has its centre
    // wherever its tallest child is — often a label — and tapping a
    // label focuses nothing, so the fill that followed had no field to
    // type into. If what was named is not itself typeable and holds
    // exactly one thing that is, that is what the caller meant.
    let aim = if named.role == Some(smix_screen::Role::TextField) {
        coord
    } else {
        match sole_text_field(named) {
            Some(field) => (
                (field.bounds.x + field.bounds.w / 2.0 - frame.x) / frame.w,
                (field.bounds.y + field.bounds.h / 2.0 - frame.y) / frame.h,
            ),
            None => coord,
        }
    };
    Ok((aim, rect))
}

/// The one typeable descendant, when there is exactly one.
///
/// Exactly one on purpose: with two, which the caller meant is a guess,
/// and a guess that types into the wrong field is the defect this whole
/// line of work is about. They can name the field itself.
fn sole_text_field(node: &smix_screen::A11yNode) -> Option<&smix_screen::A11yNode> {
    let mut found: Option<&smix_screen::A11yNode> = None;
    let mut stack: Vec<&smix_screen::A11yNode> = node.children.iter().collect();
    while let Some(n) = stack.pop() {
        if n.role == Some(smix_screen::Role::TextField) {
            if found.is_some() {
                return None;
            }
            found = Some(n);
        }
        stack.extend(n.children.iter());
    }
    found
}

pub(super) async fn resolve_with_implicit_wait(
    driver: &AndroidDriver,
    selector: &Selector,
    include: Option<IncludeScope>,
) -> Result<(f64, f64), ExpectationFailure> {
    resolve_aimed(driver, selector, include)
        .await
        .map(|(nx, ny, _, _)| (nx, ny))
}

/// The point to touch, and the element it is aimed at — read from the
/// same tree, so the element the verdict compares against is the one
/// the point was computed from.
pub(super) async fn resolve_aimed(
    driver: &AndroidDriver,
    selector: &Selector,
    include: Option<IncludeScope>,
) -> Result<crate::Aimed, ExpectationFailure> {
    let start = std::time::Instant::now();
    let timeout = Duration::from_millis(5000);
    loop {
        let perceived = driver.perceive(include).await?;
        let read_by = std::sync::Mutex::new(perceived.source);
        let tree = perceived.root;
        match resolve_to_norm_coord(&tree, selector) {
            Ok(_) => {
                // Where a touch reaches it: under an app drawn edge to edge
                // its centre can be under the status bar, which takes it.
                // Read by the same rule as every re-reading below: a target
                // that shows nowhere is not on screen yet.
                let Some(first) = crate::aim_in(&tree, selector)? else {
                    if start.elapsed() > timeout {
                        return Err(crate::element_not_found(&tree, selector));
                    }
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    continue;
                };
                // Aimed only at a target that has stopped moving (the same
                // wait as iOS). The tree is in pixels.
                let ppp = crate::Driver::pixels_per_point(driver).await?;
                let (nx, ny, aimed) = crate::settle::until_aim_settles(
                    first,
                    || async {
                        let t = driver.perceive(include).await?;
                        *read_by.lock().expect("never poisoned") = t.source;
                        crate::settle_reading(&t.root, selector)
                    },
                    ppp,
                    crate::settle::POLL,
                    crate::settle::LIMIT,
                )
                .await?;
                return Ok((
                    nx,
                    ny,
                    aimed,
                    crate::verdict_reader(*read_by.lock().expect("never poisoned")),
                ));
            }
            Err(HostResolveError::NotFound) => {
                if start.elapsed() > timeout {
                    return Err(crate::element_not_found(&tree, selector));
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }
            Err(e) => {
                return Err(ExpectationFailure::new(FailureInit {
                    code: Some(FailureCode::DriverError),
                    message: format!("AndroidDriver: resolve error: {e:?}"),
                    selector: Some(selector.clone()),
                    ..Default::default()
                }));
            }
        }
    }
}
