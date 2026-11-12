//! What `smix tree` says above the outline when the window in front is
//! the system's rather than an app's.
//!
//! The accessibility reader's caveat is about an app's Compose dialog,
//! and the cure it names is the app's probe. With the system's
//! permission dialog in front it pointed at the probe of an app that
//! already carried one, and the dialog's buttons sat at the bottom of an
//! outline whose first thirty lines were the status bar.

use smix_screen::{A11yNode, WindowKind};

/// The packages that put the runtime permission dialog on screen.
const PERMISSION_DIALOG: &[&str] = &[
    "com.google.android.permissioncontroller",
    "com.android.permissioncontroller",
];

/// The note for a screen whose focused window is the system's, or `None`
/// when an app's window holds the focus (or none says it does).
pub(crate) fn system_in_front(root: &A11yNode, device: Option<&str>) -> Option<String> {
    let front = root
        .children
        .iter()
        .find(|w| w.window.as_ref().is_some_and(|i| i.focused))?;
    let info = front.window.as_ref()?;
    let package = info.package.as_deref().unwrap_or("an unnamed package");
    let on = device.map_or_else(String::new, |d| format!(" --device {d}"));
    if PERMISSION_DIALOG.contains(&package) {
        let buttons: Vec<String> = buttons_of(front)
            .into_iter()
            .map(|b| format!("{b:?}"))
            .collect();
        let first = buttons.first().map_or("\"Allow\"", String::as_str);
        return Some(format!(
            "the window in front is the system's permission dialog ({package}), not the app's. \
             Its buttons: {}. Press one by its text: smix tap \"text:{}\"{on} — or list it with \
             `smix system-popups{on}` and press with `smix system-popup-action <popup> <button>{on}`",
            if buttons.is_empty() {
                "none readable".to_string()
            } else {
                buttons.join(", ")
            },
            first.trim_matches('"'),
        ));
    }
    let system = matches!(info.kind, WindowKind::System) || package == "android";
    system.then(|| {
        format!(
            "the window in front belongs to the system ({package}), not to an app. \
             `smix system-popups{on}` lists the dialogs smix can press"
        )
    })
}

fn buttons_of(node: &A11yNode) -> Vec<&str> {
    let mut out = Vec::new();
    if node.role == Some(smix_screen::Role::Button)
        && let Some(t) = node
            .text
            .as_deref()
            .or(node.label.as_deref())
            .filter(|t| !t.is_empty())
    {
        out.push(t);
    }
    for c in &node.children {
        out.extend(buttons_of(c));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A screen as the Android runner sends it: one child per window.
    fn screen(windows: serde_json::Value) -> A11yNode {
        let root: A11yNode = serde_json::from_value(serde_json::json!({
            "rawType": "screen", "elementTypeRaw": 0,
            "bounds": {"x": 0, "y": 0, "w": 1080, "h": 2400},
            "enabled": true, "selected": false, "hasFocus": false, "visible": true,
            "children": windows,
        }))
        .expect("a screen");
        root
    }

    fn window(
        package: &str,
        kind: &str,
        focused: bool,
        children: serde_json::Value,
    ) -> serde_json::Value {
        serde_json::json!({
            "rawType": "window", "elementTypeRaw": 0,
            "bounds": {"x": 0, "y": 0, "w": 1080, "h": 2400},
            "enabled": true, "selected": false, "hasFocus": false, "visible": true,
            "window": {"package": package, "kind": kind, "focused": focused},
            "children": children,
        })
    }

    fn button(text: &str) -> serde_json::Value {
        serde_json::json!({
            "rawType": "android.widget.Button", "role": "button", "elementTypeRaw": 0, "text": text,
            "bounds": {"x": 0, "y": 0, "w": 100, "h": 50},
            "enabled": true, "selected": false, "hasFocus": false, "visible": true,
            "children": [],
        })
    }

    fn status_bar() -> serde_json::Value {
        window(
            "com.android.systemui",
            "system",
            false,
            serde_json::json!([]),
        )
    }

    #[test]
    fn the_permission_dialog_in_front_is_named_with_its_buttons() {
        let root = screen(serde_json::json!([
            status_bar(),
            window(
                "com.google.android.permissioncontroller",
                "application",
                true,
                serde_json::json!([button("While using the app"), button("Don\u{2019}t allow")]),
            ),
        ]));
        let note = system_in_front(&root, Some("emulator-5554")).expect("a note");
        assert!(note.contains("system's permission dialog"), "{note}");
        assert!(note.contains("\"Don\u{2019}t allow\""), "{note}");
        assert!(
            note.contains("smix tap \"text:While using the app\" --device emulator-5554"),
            "{note}"
        );
        assert!(
            note.contains("smix system-popups --device emulator-5554"),
            "{note}"
        );
    }

    #[test]
    fn an_app_in_front_gets_no_note() {
        let root = screen(serde_json::json!([
            status_bar(),
            window(
                "dev.example.app",
                "application",
                true,
                serde_json::json!([button("OK")])
            ),
        ]));
        assert_eq!(system_in_front(&root, None), None);
    }

    #[test]
    fn a_system_window_in_front_points_at_system_popups() {
        let root = screen(serde_json::json!([window(
            "com.android.systemui",
            "system",
            true,
            serde_json::json!([])
        )]));
        let note = system_in_front(&root, None).expect("a note");
        assert!(
            note.contains("belongs to the system (com.android.systemui)"),
            "{note}"
        );
        assert!(note.contains("`smix system-popups`"), "{note}");
    }

    #[test]
    fn no_focused_window_gets_no_note() {
        assert_eq!(
            system_in_front(&screen(serde_json::json!([status_bar()])), None),
            None
        );
    }
}
