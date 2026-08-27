//! Unit tests for `role_from_raw_type` + `derive_roles_recursive`.
//!
//! The Swift `/tree` route only emits `rawType` on the wire (never `role`),
//! so the runner-client post-deser pass uses these helpers to lift each
//! node's `rawType` string into a curated [`Role`]. Tests cover the
//! identity-mapped cases ("button" → Button), the name-mismatch cases
//! ("radioButton" → Radio, "progressIndicator" → ProgressBar), the
//! intentional gaps ("other", "any", "window", "group" → None), the
//! host-set override (existing `role` is preserved), and the recursive
//! walk.

use smix_screen::{A11yNode, Rect, Role, derive_roles_recursive, role_from_raw_type};

fn rect() -> Rect {
    Rect {
        x: 0.0,
        y: 0.0,
        w: 10.0,
        h: 10.0,
    }
}

fn mk(raw_type: &str, role: Option<Role>, children: Vec<A11yNode>) -> A11yNode {
    A11yNode {
        hittable: None,
        raw_type: raw_type.into(),
        element_type_raw: 1,
        role,
        identifier: None,
        label: None,
        title: None,
        placeholder_value: None,
        value: None,
        text: None,
        bounds: rect(),
        enabled: true,
        selected: false,
        has_focus: false,
        visible: true,
        children,
    }
}

#[test]
fn role_from_raw_type_identity_mapped() {
    assert_eq!(role_from_raw_type("button"), Some(Role::Button));
    assert_eq!(role_from_raw_type("link"), Some(Role::Link));
    assert_eq!(role_from_raw_type("textField"), Some(Role::TextField));
    assert_eq!(
        role_from_raw_type("secureTextField"),
        Some(Role::SecureTextField)
    );
    assert_eq!(role_from_raw_type("searchField"), Some(Role::SearchField));
    assert_eq!(role_from_raw_type("switch"), Some(Role::Switch));
    assert_eq!(role_from_raw_type("staticText"), Some(Role::StaticText));
    assert_eq!(role_from_raw_type("alert"), Some(Role::Alert));
    assert_eq!(role_from_raw_type("dialog"), Some(Role::Dialog));
    assert_eq!(role_from_raw_type("cell"), Some(Role::Cell));
    assert_eq!(role_from_raw_type("tabBar"), Some(Role::TabBar));
    assert_eq!(
        role_from_raw_type("navigationBar"),
        Some(Role::NavigationBar)
    );
    assert_eq!(role_from_raw_type("scrollView"), Some(Role::ScrollView));
    assert_eq!(
        role_from_raw_type("segmentedControl"),
        Some(Role::SegmentedControl)
    );
    assert_eq!(
        role_from_raw_type("collectionView"),
        Some(Role::CollectionView)
    );
    assert_eq!(role_from_raw_type("webView"), Some(Role::WebView));
    assert_eq!(role_from_raw_type("keyboard"), Some(Role::Keyboard));
}

#[test]
fn role_from_raw_type_name_mismatches() {
    // Swift wire names differ from Rust enum variant names — these are
    // the regression-cover cases where a naive identity map would lose
    // the role.
    assert_eq!(role_from_raw_type("radioButton"), Some(Role::Radio));
    assert_eq!(
        role_from_raw_type("progressIndicator"),
        Some(Role::ProgressBar)
    );
}

#[test]
fn role_from_raw_type_returns_none_for_chrome_types() {
    // These swift raw types have no curated semantic in Rust — they're
    // intentionally None so Selector::Role won't accidentally match
    // structural chrome.
    assert_eq!(role_from_raw_type("any"), None);
    assert_eq!(role_from_raw_type("other"), None);
    assert_eq!(role_from_raw_type("application"), None);
    assert_eq!(role_from_raw_type("window"), None);
    assert_eq!(role_from_raw_type("group"), None);
    assert_eq!(role_from_raw_type("sheet"), None);
    assert_eq!(role_from_raw_type("drawer"), None);
    assert_eq!(role_from_raw_type("popover"), None);
    assert_eq!(role_from_raw_type(""), None);
    assert_eq!(role_from_raw_type("unknownGarbage"), None);
}

#[test]
fn derive_roles_recursive_fills_missing_roles_only() {
    let mut tree = mk(
        "alert",
        None,
        vec![
            mk("button", None, vec![]),
            // Host-set role should NOT be overwritten even though the
            // mapping would yield a different Role.
            mk("button", Some(Role::Link), vec![]),
            mk("staticText", None, vec![mk("radioButton", None, vec![])]),
        ],
    );
    derive_roles_recursive(&mut tree);

    assert_eq!(tree.role, Some(Role::Alert));
    assert_eq!(tree.children[0].role, Some(Role::Button));
    // Override preserved.
    assert_eq!(tree.children[1].role, Some(Role::Link));
    assert_eq!(tree.children[2].role, Some(Role::StaticText));
    // Recursive grandchild.
    assert_eq!(tree.children[2].children[0].role, Some(Role::Radio));
}

#[test]
fn derive_roles_recursive_leaves_unknown_raw_types_as_none() {
    let mut tree = mk(
        "application",
        None,
        vec![mk("window", None, vec![mk("group", None, vec![])])],
    );
    derive_roles_recursive(&mut tree);
    assert_eq!(tree.role, None);
    assert_eq!(tree.children[0].role, None);
    assert_eq!(tree.children[0].children[0].role, None);
}

// iOS UITabBar items are `button`s nested (via wrapper `other` nodes)
// inside a `tabBar` subtree; there is NO distinct tab ElementType
// (swift `elementTypeName` has no "tab" case). Structural inference:
// any button anywhere inside a tabBar subtree → Role::Tab.
#[test]
fn derive_roles_button_inside_tab_bar_subtree_becomes_tab() {
    let mut tree = mk(
        "window",
        None,
        vec![
            // a button OUTSIDE any tab bar stays a plain Button
            mk("button", None, vec![]),
            mk(
                "tabBar",
                None,
                // real tree nests the tab buttons under wrapper `other` nodes,
                // so the inference must be ancestor-based, not direct-parent.
                vec![mk(
                    "other",
                    None,
                    vec![mk(
                        "other",
                        None,
                        vec![mk("button", None, vec![]), mk("button", None, vec![])],
                    )],
                )],
            ),
        ],
    );
    derive_roles_recursive(&mut tree);

    // plain button outside tabBar
    assert_eq!(tree.children[0].role, Some(Role::Button));
    // the tabBar container itself
    assert_eq!(tree.children[1].role, Some(Role::TabBar));
    // buttons nested (через 2 `other` wrappers) inside the tabBar subtree
    let inner = &tree.children[1].children[0].children[0];
    assert_eq!(inner.children[0].role, Some(Role::Tab));
    assert_eq!(inner.children[1].role, Some(Role::Tab));
}

#[test]
fn derive_roles_tab_bar_inference_respects_host_set_role() {
    // Host-set role inside a tab bar is NOT overwritten by the Tab inference
    // (same is_none() contract as the rest of derive_roles_recursive).
    let mut tree = mk(
        "tabBar",
        None,
        vec![mk("button", Some(Role::Button), vec![])],
    );
    derive_roles_recursive(&mut tree);
    assert_eq!(tree.children[0].role, Some(Role::Button));
}
