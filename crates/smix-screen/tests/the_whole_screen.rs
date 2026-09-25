//! A tree read through the probe is still the whole screen.
//!
//! The probe reads the app from inside its process and sees it better than
//! the accessibility projection does. It sees nothing else: not the
//! keyboard, not the status bar, not another app's dialog on top. Handing
//! a caller the probe's tree alone made `role:keyboard` unanswerable on any
//! app that carried the probe, with the keyboard plainly on screen
//! (2026-09-25, `the-three-that-went-red` #3, three runs out of three).

use smix_screen::{A11yNode, Role, WindowInfo, WindowKind, beside_other_windows};

fn node(json: &str) -> A11yNode {
    serde_json::from_str(json).expect("the fixture node parses")
}

fn window(package: &str, kind: &str, focused: bool, id: &str, role: Option<&str>) -> String {
    let role = role
        .map(|r| format!(r#","role":"{r}""#))
        .unwrap_or_default();
    format!(
        r#"{{"rawType":"android.widget.FrameLayout","identifier":"{id}"{role},
            "window":{{"package":"{package}","kind":"{kind}","focused":{focused}}},
            "bounds":{{"x":0.0,"y":0.0,"w":1080.0,"h":2340.0}},
            "enabled":true,"selected":false,"hasFocus":false,"visible":true,"children":[]}}"#
    )
}

/// The accessibility tree as the Android runner sends it: a root holding
/// one child per window, top of the stack first.
fn accessibility(windows: &[String]) -> A11yNode {
    node(&format!(
        r#"{{"rawType":"android.view.WindowRoot",
            "bounds":{{"x":0.0,"y":0.0,"w":1080.0,"h":2340.0}},
            "enabled":true,"selected":false,"hasFocus":false,"visible":true,
            "children":[{}]}}"#,
        windows.join(",")
    ))
}

fn probe_tree() -> A11yNode {
    node(
        r#"{"rawType":"SemanticsRoots",
            "bounds":{"x":0.0,"y":0.0,"w":1080.0,"h":2340.0},
            "enabled":true,"selected":false,"hasFocus":false,"visible":true,
            "children":[{"rawType":"TextField","identifier":"compose_input",
              "bounds":{"x":40.0,"y":200.0,"w":800.0,"h":150.0},
              "enabled":true,"selected":false,"hasFocus":true,"visible":true,
              "children":[]}]}"#,
    )
}

fn keyboard_screen() -> A11yNode {
    accessibility(&[
        window(
            "com.android.systemui",
            "system",
            false,
            "navigation_bar_frame",
            None,
        ),
        window(
            "com.android.inputmethod.latin",
            "inputMethod",
            false,
            "ime",
            Some("keyboard"),
        ),
        window("dev.smix.fixture", "application", true, "app_decor", None),
    ])
}

fn ids(root: &A11yNode) -> Vec<String> {
    root.children
        .iter()
        .map(|c| c.identifier.clone().unwrap_or_else(|| c.raw_type.clone()))
        .collect()
}

#[test]
fn the_keyboard_survives_the_probe() {
    let merged = beside_other_windows(keyboard_screen(), probe_tree(), Some("dev.smix.fixture"));
    let keyboards = merged
        .children
        .iter()
        .filter(|c| c.role == Some(Role::Keyboard))
        .count();
    assert_eq!(
        keyboards,
        1,
        "the keyboard window is gone: {:?}",
        ids(&merged)
    );
}

#[test]
fn the_app_window_is_the_probes_and_keeps_its_place() {
    let merged = beside_other_windows(keyboard_screen(), probe_tree(), Some("dev.smix.fixture"));
    // Same order as the accessibility tree: the stack is what a reader
    // of "what is on top" is asking about.
    assert_eq!(
        ids(&merged),
        vec!["navigation_bar_frame", "ime", "SemanticsRoots"],
        "the app's accessibility window must be replaced in place"
    );
    let app = &merged.children[2];
    assert_eq!(
        app.window,
        Some(WindowInfo {
            package: Some("dev.smix.fixture".into()),
            kind: WindowKind::Application,
            focused: true,
        }),
        "the probe's subtree must say whose window it is"
    );
    assert_eq!(app.children[0].identifier.as_deref(), Some("compose_input"));
}

#[test]
fn every_window_of_the_app_is_the_probes() {
    // The probe reads every window of the app's process (a dialog of the
    // app's own is one more window), so no accessibility copy of any of
    // them may stay beside it — that would put each control in twice.
    let screen = accessibility(&[
        window("dev.smix.fixture", "application", true, "app_dialog", None),
        window("com.android.systemui", "system", false, "status_bar", None),
        window("dev.smix.fixture", "application", false, "app_decor", None),
    ]);
    let merged = beside_other_windows(screen, probe_tree(), Some("dev.smix.fixture"));
    assert_eq!(ids(&merged), vec!["SemanticsRoots", "status_bar"]);
}

#[test]
fn another_apps_window_is_not_the_probes() {
    let screen = accessibility(&[
        window(
            "com.android.permissioncontroller",
            "application",
            true,
            "grant_dialog",
            None,
        ),
        window("dev.smix.fixture", "application", false, "app_decor", None),
    ]);
    let merged = beside_other_windows(screen, probe_tree(), Some("dev.smix.fixture"));
    assert_eq!(ids(&merged), vec!["grant_dialog", "SemanticsRoots"]);
}

#[test]
fn the_root_is_the_screen() {
    // Every tap is a share of the root's rectangle; the screen, not the
    // app's part of it.
    let merged = beside_other_windows(keyboard_screen(), probe_tree(), Some("dev.smix.fixture"));
    assert_eq!(merged.raw_type, "android.view.WindowRoot");
    assert_eq!((merged.bounds.w, merged.bounds.h), (1080.0, 2340.0));
}

#[test]
fn a_screen_without_the_apps_window_keeps_the_probes_tree_last() {
    // Mid-transition the accessibility reader may not list the app yet.
    // What the probe saw is still the app; it goes to the bottom of the
    // stack, where an app's window is.
    let screen = accessibility(&[window(
        "com.android.inputmethod.latin",
        "inputMethod",
        false,
        "ime",
        Some("keyboard"),
    )]);
    let merged = beside_other_windows(screen, probe_tree(), Some("dev.smix.fixture"));
    assert_eq!(ids(&merged), vec!["ime", "SemanticsRoots"]);
}

#[test]
fn a_tree_without_windows_has_nothing_to_add() {
    // iOS, and any accessibility tree that is one app's rather than a
    // stack of windows: nothing beside the app to keep.
    let one_app = node(
        r#"{"rawType":"application","identifier":"root",
            "bounds":{"x":0.0,"y":0.0,"w":390.0,"h":844.0},
            "enabled":true,"selected":false,"hasFocus":false,"visible":true,
            "children":[]}"#,
    );
    let merged = beside_other_windows(one_app, probe_tree(), Some("dev.smix.fixture"));
    assert_eq!(merged.raw_type, "SemanticsRoots");
}

#[test]
fn an_unnamed_app_is_the_one_holding_the_focus() {
    // A CLI verb names no app, and the runner then probes the app whose
    // window holds the focus — so that is the window the probe's tree
    // stands for, and an unfocused app behind it is still somebody else.
    let screen = accessibility(&[
        window(
            "com.android.inputmethod.latin",
            "inputMethod",
            false,
            "ime",
            Some("keyboard"),
        ),
        window("dev.smix.fixture", "application", true, "app_decor", None),
        window("com.example.behind", "application", false, "behind", None),
    ]);
    let merged = beside_other_windows(screen, probe_tree(), None);
    assert_eq!(ids(&merged), vec!["ime", "SemanticsRoots", "behind"]);
    assert_eq!(
        merged.children[1]
            .window
            .as_ref()
            .and_then(|w| w.package.as_deref()),
        Some("dev.smix.fixture")
    );
}
