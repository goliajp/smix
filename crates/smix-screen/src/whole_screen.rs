//! The screen, read through the probe.
//!
//! The probe reads the app from inside its process and sees it better than
//! the accessibility projection does — a Compose dialog, a hosted View, a
//! node's real placement. It sees nothing that is not the app: the
//! keyboard, the system bars, another app's dialog on top. Handing a
//! caller the probe's tree alone made those absent rather than unread, and
//! `role:keyboard` timed out on every app that carried the probe while the
//! keyboard was on screen.
//!
//! So a tree read through the probe is composed: the app's windows from the
//! probe, every other window from the accessibility reader, each saying
//! whose it is.

use crate::{A11yNode, WindowInfo, WindowKind};

/// The whole screen: `app_tree` in place of the app's windows, the rest of
/// `accessibility` around it.
///
/// `accessibility` is the reader's tree as the Android runner sends it — a
/// root whose children are the windows, top of the stack first, each
/// carrying a [`WindowInfo`]. A tree whose children carry none (iOS, or one
/// app's tree rather than a stack) has nothing beside the app to add, and
/// `app_tree` is returned as it is.
///
/// `app` is the package the probe answered about. When the caller does not
/// know it, it is the application window holding the focus: that is the one
/// the runner probes when nobody names an app.
///
/// Every window of that package is replaced — the probe reads every window
/// of the app's process, so an accessibility copy left beside it would put
/// each control in twice. The probe's tree takes the place of the topmost of
/// them, or the bottom of the stack when the reader listed none (an app's
/// window is under the system's), and says it is the app's.
///
/// The root is the accessibility root: the screen, which every tap is a
/// share of.
pub fn beside_other_windows(
    accessibility: A11yNode,
    mut app_tree: A11yNode,
    app: Option<&str>,
) -> A11yNode {
    if !accessibility.children.iter().any(|c| c.window.is_some()) {
        return app_tree;
    }
    let app = app
        .map(str::to_owned)
        .or_else(|| focused_application(&accessibility));
    let mut root = accessibility;
    let windows = std::mem::take(&mut root.children);
    let mut place = None;
    let mut focused = false;
    for w in windows {
        if is_the_apps(&w, app.as_deref()) {
            place.get_or_insert(root.children.len());
            focused |= w.window.as_ref().is_some_and(|i| i.focused);
        } else {
            root.children.push(w);
        }
    }
    app_tree.window = Some(WindowInfo {
        package: app,
        kind: WindowKind::Application,
        // No window of the app listed means the reader has not caught up
        // with it; the probe answered, so it is the app in front.
        focused: focused || place.is_none(),
    });
    let at = place.unwrap_or(root.children.len());
    root.children.insert(at, app_tree);
    root
}

fn is_the_apps(window: &A11yNode, app: Option<&str>) -> bool {
    let Some(info) = &window.window else {
        return false;
    };
    match info.kind {
        WindowKind::Application => app.is_some() && info.package.as_deref() == app,
        WindowKind::InputMethod | WindowKind::System | WindowKind::Other => false,
    }
}

fn focused_application(accessibility: &A11yNode) -> Option<String> {
    accessibility
        .children
        .iter()
        .filter_map(|c| c.window.as_ref())
        .find(|w| w.focused && w.kind == WindowKind::Application)
        .and_then(|w| w.package.clone())
}
