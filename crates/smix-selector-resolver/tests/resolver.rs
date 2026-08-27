//! Unit tests for smix-selector-resolver.
//!
//! Coverage:
//! - text/id/label/role base forms + 6 spatial filters + index pick +
//!   zero-bounds + boundary.
//! - Anchor-only base form (cases F-K).

use smix_screen::{A11yNode, Rect, Role};
use smix_selector::{AnchorBox, IndexModifiers, Modifiers, Pattern, Selector, True};
use smix_selector_resolver::{resolve_selector, resolve_selector_all};

// ---------- node builders ----------

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect { x, y, w, h }
}

#[derive(Default, Clone)]
struct NodePartial {
    raw_type: Option<String>,
    #[allow(dead_code)]
    element_type_raw: Option<u64>,
    role: Option<Role>,
    identifier: Option<String>,
    label: Option<String>,
    title: Option<String>,
    placeholder_value: Option<String>,
    value: Option<String>,
    text: Option<String>,
    bounds: Option<Rect>,
    has_focus: Option<bool>,
    children: Option<Vec<A11yNode>>,
}

fn mk(p: NodePartial) -> A11yNode {
    A11yNode {
        hittable: None,
        raw_type: p.raw_type.unwrap_or_else(|| "other".into()),
        element_type_raw: 1,
        role: p.role,
        identifier: p.identifier,
        label: p.label,
        title: p.title,
        placeholder_value: p.placeholder_value,
        value: p.value,
        text: p.text,
        bounds: p.bounds.unwrap_or(rect(0.0, 0.0, 10.0, 10.0)),
        enabled: true,
        selected: false,
        has_focus: p.has_focus.unwrap_or(false),
        visible: true,
        children: p.children.unwrap_or_default(),
    }
}

fn root(children: Vec<A11yNode>, bounds: Rect) -> A11yNode {
    mk(NodePartial {
        raw_type: Some("application".into()),
        bounds: Some(bounds),
        children: Some(children),
        ..Default::default()
    })
}

fn text_sel(t: &str) -> Selector {
    Selector::Text {
        text: Pattern::text(t),
        modifiers: Modifiers::default(),
    }
}

fn text_regex(p: &str) -> Selector {
    Selector::Text {
        text: Pattern::regex(p),
        modifiers: Modifiers::default(),
    }
}

fn id_sel(i: &str) -> Selector {
    Selector::Id {
        id: i.to_string(),
        modifiers: Modifiers::default(),
    }
}

fn label_sel(l: &str) -> Selector {
    Selector::Label {
        label: l.to_string(),
        modifiers: Modifiers::default(),
    }
}

// ---- text base ----------------------------------------------------------

#[test]
fn text_string_label_hit() {
    let n = mk(NodePartial {
        label: Some("Settings".into()),
        ..Default::default()
    });
    let tree = root(vec![n.clone()], rect(0.0, 0.0, 390.0, 844.0));
    let got = resolve_selector(&tree, &text_sel("settings"));
    assert!(got.is_some());
    assert_eq!(got.unwrap().label.as_deref(), Some("Settings"));
}

#[test]
fn text_string_miss_returns_none() {
    let n = mk(NodePartial {
        label: Some("Dashboard".into()),
        ..Default::default()
    });
    let tree = root(vec![n], rect(0.0, 0.0, 390.0, 844.0));
    assert!(resolve_selector(&tree, &text_sel("Settings")).is_none());
}

#[test]
fn text_string_empty_returns_none() {
    let n = mk(NodePartial {
        label: Some("X".into()),
        ..Default::default()
    });
    let tree = root(vec![n], rect(0.0, 0.0, 390.0, 844.0));
    assert!(resolve_selector(&tree, &text_sel("")).is_none());
}

#[test]
fn text_regex_default_auto_i() {
    let n = mk(NodePartial {
        label: Some("Hello".into()),
        ..Default::default()
    });
    let tree = root(vec![n], rect(0.0, 0.0, 390.0, 844.0));
    assert!(resolve_selector(&tree, &text_regex("^hello")).is_some());
}

#[test]
fn text_regex_partial_match() {
    let n = mk(NodePartial {
        label: Some("Hello world".into()),
        ..Default::default()
    });
    let tree = root(vec![n], rect(0.0, 0.0, 390.0, 844.0));
    assert!(resolve_selector(&tree, &text_regex("^Hel")).is_some());
}

#[test]
fn text_regex_invalid_returns_none() {
    // Unbalanced bracket — regex compile error.
    let n = mk(NodePartial {
        label: Some("X".into()),
        ..Default::default()
    });
    let tree = root(vec![n], rect(0.0, 0.0, 390.0, 844.0));
    assert!(resolve_selector(&tree, &text_regex("[")).is_none());
}

// ---- id / label / role base ---------------------------------------------

#[test]
fn id_strict_equal_hit() {
    let n = mk(NodePartial {
        identifier: Some("btn-x".into()),
        ..Default::default()
    });
    let tree = root(vec![n], rect(0.0, 0.0, 390.0, 844.0));
    assert!(resolve_selector(&tree, &id_sel("btn-x")).is_some());
}

#[test]
fn id_strict_equal_case_sensitive() {
    let n = mk(NodePartial {
        identifier: Some("btn-x".into()),
        ..Default::default()
    });
    let tree = root(vec![n], rect(0.0, 0.0, 390.0, 844.0));
    // id is case-sensitive (`node.identifier === selector.id`).
    assert!(resolve_selector(&tree, &id_sel("BTN-X")).is_none());
}

#[test]
fn id_empty_returns_none() {
    let n = mk(NodePartial {
        identifier: Some("X".into()),
        ..Default::default()
    });
    let tree = root(vec![n], rect(0.0, 0.0, 390.0, 844.0));
    assert!(resolve_selector(&tree, &id_sel("")).is_none());
}

#[test]
fn label_strict_equal_case_sensitive_hit() {
    let n = mk(NodePartial {
        label: Some("Settings".into()),
        ..Default::default()
    });
    let tree = root(vec![n], rect(0.0, 0.0, 390.0, 844.0));
    assert!(resolve_selector(&tree, &label_sel("Settings")).is_some());
    // case-sensitive.
    assert!(resolve_selector(&tree, &label_sel("settings")).is_none());
}

#[test]
fn role_match_no_name() {
    let n = mk(NodePartial {
        role: Some(Role::Button),
        ..Default::default()
    });
    let tree = root(vec![n], rect(0.0, 0.0, 390.0, 844.0));
    let s = Selector::Role {
        role: Role::Button,
        name: None,
        modifiers: Modifiers::default(),
    };
    assert!(resolve_selector(&tree, &s).is_some());
}

#[test]
fn role_match_with_name_pattern() {
    let n = mk(NodePartial {
        role: Some(Role::Button),
        label: Some("Submit".into()),
        ..Default::default()
    });
    let tree = root(vec![n], rect(0.0, 0.0, 390.0, 844.0));
    let s = Selector::Role {
        role: Role::Button,
        name: Some(Pattern::text("submit")),
        modifiers: Modifiers::default(),
    };
    assert!(resolve_selector(&tree, &s).is_some());
}

#[test]
fn role_mismatch_returns_none() {
    let n = mk(NodePartial {
        role: Some(Role::Button),
        ..Default::default()
    });
    let tree = root(vec![n], rect(0.0, 0.0, 390.0, 844.0));
    let s = Selector::Role {
        role: Role::TextField,
        name: None,
        modifiers: Modifiers::default(),
    };
    assert!(resolve_selector(&tree, &s).is_none());
}

// ---- focused base -------------------------------------------------------

#[test]
fn focused_hits_node_with_has_focus() {
    let n = mk(NodePartial {
        label: Some("Focused".into()),
        has_focus: Some(true),
        ..Default::default()
    });
    let tree = root(vec![n], rect(0.0, 0.0, 390.0, 844.0));
    let got = resolve_selector(
        &tree,
        &Selector::Focused {
            focused: True(true),
        },
    );
    assert!(got.is_some());
    assert_eq!(got.unwrap().label.as_deref(), Some("Focused"));
}

#[test]
fn focused_miss_returns_none_when_no_focus() {
    let n = mk(NodePartial {
        label: Some("Plain".into()),
        ..Default::default()
    });
    let tree = root(vec![n], rect(0.0, 0.0, 390.0, 844.0));
    assert!(
        resolve_selector(
            &tree,
            &Selector::Focused {
                focused: True(true)
            }
        )
        .is_none()
    );
}

// ---- visibility filter --------------------------------------------------

#[test]
fn zero_bounds_node_filtered_out() {
    let visible_node = mk(NodePartial {
        label: Some("V".into()),
        bounds: Some(rect(10.0, 10.0, 10.0, 10.0)),
        ..Default::default()
    });
    let zero_node = mk(NodePartial {
        label: Some("V".into()),
        bounds: Some(rect(0.0, 0.0, 0.0, 0.0)),
        ..Default::default()
    });
    let tree = root(
        vec![zero_node, visible_node.clone()],
        rect(0.0, 0.0, 390.0, 844.0),
    );
    let got = resolve_selector(&tree, &text_sel("V")).expect("hit");
    // The visibility filter drops the zero-bounds candidate;
    // pre-order survivor is the visible node.
    assert_eq!(got.bounds, rect(10.0, 10.0, 10.0, 10.0));
}

// ---- spatial filter -----------------------------------------------------

#[test]
fn spatial_below_hit() {
    let anchor = mk(NodePartial {
        label: Some("Anchor".into()),
        bounds: Some(rect(0.0, 0.0, 10.0, 10.0)),
        ..Default::default()
    });
    let cand = mk(NodePartial {
        label: Some("Below".into()),
        bounds: Some(rect(0.0, 100.0, 10.0, 10.0)),
        ..Default::default()
    });
    let tree = root(vec![anchor, cand], rect(-1000.0, -1000.0, 2000.0, 2000.0));
    let s = Selector::Text {
        text: Pattern::text("Below"),
        modifiers: Modifiers {
            below: Some(Box::new(text_sel("Anchor"))),
            ..Default::default()
        },
    };
    let got = resolve_selector(&tree, &s).expect("hit");
    assert_eq!(got.label.as_deref(), Some("Below"));
}

#[test]
fn spatial_anchor_null_short_circuits() {
    let cand = mk(NodePartial {
        label: Some("Below".into()),
        bounds: Some(rect(0.0, 100.0, 10.0, 10.0)),
        ..Default::default()
    });
    let tree = root(vec![cand], rect(-1000.0, -1000.0, 2000.0, 2000.0));
    let s = Selector::Text {
        text: Pattern::text("Below"),
        modifiers: Modifiers {
            below: Some(Box::new(text_sel("DoesNotExist"))),
            ..Default::default()
        },
    };
    // Anchor resolves to None → overall None.
    assert!(resolve_selector(&tree, &s).is_none());
}

#[test]
fn spatial_near_boundary_100pt_is_hit() {
    let anchor = mk(NodePartial {
        label: Some("Anchor".into()),
        bounds: Some(rect(0.0, 0.0, 1.0, 1.0)), // centroid (0.5, 0.5)
        ..Default::default()
    });
    let cand = mk(NodePartial {
        label: Some("Target".into()),
        bounds: Some(rect(100.0, 0.0, 1.0, 1.0)), // centroid (100.5, 0.5); dist=100
        ..Default::default()
    });
    let tree = root(vec![anchor, cand], rect(0.0, 0.0, 500.0, 500.0));
    let s = Selector::Text {
        text: Pattern::text("Target"),
        modifiers: Modifiers {
            near: Some(Box::new(text_sel("Anchor"))),
            ..Default::default()
        },
    };
    assert!(resolve_selector(&tree, &s).is_some());
}

#[test]
fn spatial_near_just_over_100pt_misses() {
    let anchor = mk(NodePartial {
        label: Some("Anchor".into()),
        bounds: Some(rect(0.0, 0.0, 1.0, 1.0)),
        ..Default::default()
    });
    let cand = mk(NodePartial {
        label: Some("Target".into()),
        bounds: Some(rect(100.01, 0.0, 1.0, 1.0)),
        ..Default::default()
    });
    let tree = root(vec![anchor, cand], rect(0.0, 0.0, 500.0, 500.0));
    let s = Selector::Text {
        text: Pattern::text("Target"),
        modifiers: Modifiers {
            near: Some(Box::new(text_sel("Anchor"))),
            ..Default::default()
        },
    };
    assert!(resolve_selector(&tree, &s).is_none());
}

#[test]
fn spatial_inside_geometric_containment() {
    let outer = mk(NodePartial {
        label: Some("Outer".into()),
        bounds: Some(rect(0.0, 0.0, 200.0, 200.0)),
        ..Default::default()
    });
    let inside_node = mk(NodePartial {
        label: Some("Inside".into()),
        bounds: Some(rect(50.0, 50.0, 10.0, 10.0)),
        ..Default::default()
    });
    let tree = root(vec![outer, inside_node], rect(0.0, 0.0, 500.0, 500.0));
    let s = Selector::Text {
        text: Pattern::text("Inside"),
        modifiers: Modifiers {
            inside: Some(Box::new(text_sel("Outer"))),
            ..Default::default()
        },
    };
    assert!(resolve_selector(&tree, &s).is_some());
}

// ---- ancestor modifier -------------------------------------------------

fn role_sel(r: Role) -> Selector {
    Selector::Role {
        role: r,
        name: None,
        modifiers: Modifiers::default(),
    }
}

#[test]
fn ancestor_picks_candidate_whose_parent_chain_contains_ancestor_subselector() {
    // tree (content before tab_bar — so DFS pre-order reaches
    // sub_tab_tracking first; if the ancestor filter did not fire, the
    // first DFS hit would be sub_tab_tracking rather than
    // bottom_tab_tracking, letting the fixture distinguish the two).
    //   root (app)
    //   ├── content (DFS first)
    //   │   └── sub_tab_tracking (raw_type=tab, label="Tracking", bounds=(50,150,...))
    //   └── tab_bar (role=TabBar)
    //       └── bottom_tab_tracking (raw_type=tab, label="Tracking", bounds=(50,800,...))
    // selector = text("Tracking") + ancestor=role(TabBar) → bottom_tab_tracking
    let sub_tab_tracking = mk(NodePartial {
        raw_type: Some("tab".into()),
        label: Some("Tracking".into()),
        bounds: Some(rect(50.0, 150.0, 80.0, 40.0)),
        ..Default::default()
    });
    let content = mk(NodePartial {
        raw_type: Some("other".into()),
        bounds: Some(rect(0.0, 100.0, 400.0, 600.0)),
        children: Some(vec![sub_tab_tracking]),
        ..Default::default()
    });
    let bottom_tab_tracking = mk(NodePartial {
        raw_type: Some("tab".into()),
        label: Some("Tracking".into()),
        bounds: Some(rect(50.0, 800.0, 80.0, 40.0)),
        ..Default::default()
    });
    let tab_bar = mk(NodePartial {
        raw_type: Some("tabBar".into()),
        role: Some(Role::TabBar),
        bounds: Some(rect(0.0, 780.0, 400.0, 80.0)),
        children: Some(vec![bottom_tab_tracking]),
        ..Default::default()
    });
    let tree = root(vec![content, tab_bar], rect(0.0, 0.0, 400.0, 900.0));

    let s = Selector::Text {
        text: Pattern::text("Tracking"),
        modifiers: Modifiers {
            ancestor: Some(Box::new(role_sel(Role::TabBar))),
            ..Default::default()
        },
    };
    let picked = resolve_selector(&tree, &s).expect("should hit bottom_tab_tracking");
    assert_eq!(
        picked.bounds.y, 800.0,
        "should hit bottom_tab_tracking (ancestor chain contains TabBar; DFS first is sub_tab_tracking @ y=150, ancestor filter must fire to pull down to y=800), actually hit bounds.y={}",
        picked.bounds.y
    );
}

#[test]
fn ancestor_no_match_returns_none() {
    // Same tree shape, ancestor swapped to a non-existent id → the
    // resolve short-circuits to None.
    let bottom_tab = mk(NodePartial {
        raw_type: Some("tab".into()),
        label: Some("Tracking".into()),
        ..Default::default()
    });
    let tab_bar = mk(NodePartial {
        raw_type: Some("tabBar".into()),
        role: Some(Role::TabBar),
        children: Some(vec![bottom_tab]),
        ..Default::default()
    });
    let tree = root(vec![tab_bar], rect(0.0, 0.0, 400.0, 900.0));

    let s = Selector::Text {
        text: Pattern::text("Tracking"),
        modifiers: Modifiers {
            ancestor: Some(Box::new(id_sel("nonexistent-id"))),
            ..Default::default()
        },
    };
    assert!(
        resolve_selector(&tree, &s).is_none(),
        "ancestor sub-selector resolves to nothing → the whole selector must short-circuit to None"
    );
}

#[test]
fn ancestor_multi_candidates_same_ancestor_keeps_all_for_resolve_all() {
    // Two under tab_bar + one outside → total = 3.
    // With ancestor=role(TabBar) filter, resolve_all = 2; without it = 3.
    let tab_a = mk(NodePartial {
        raw_type: Some("tab".into()),
        label: Some("Same".into()),
        bounds: Some(rect(0.0, 800.0, 80.0, 40.0)),
        ..Default::default()
    });
    let tab_b = mk(NodePartial {
        raw_type: Some("tab".into()),
        label: Some("Same".into()),
        bounds: Some(rect(80.0, 800.0, 80.0, 40.0)),
        ..Default::default()
    });
    let tab_bar = mk(NodePartial {
        raw_type: Some("tabBar".into()),
        role: Some(Role::TabBar),
        bounds: Some(rect(0.0, 780.0, 400.0, 80.0)),
        children: Some(vec![tab_a, tab_b]),
        ..Default::default()
    });
    let stray_same = mk(NodePartial {
        raw_type: Some("tab".into()),
        label: Some("Same".into()),
        bounds: Some(rect(200.0, 100.0, 80.0, 40.0)),
        ..Default::default()
    });
    let other_section = mk(NodePartial {
        raw_type: Some("other".into()),
        bounds: Some(rect(0.0, 50.0, 400.0, 200.0)),
        children: Some(vec![stray_same]),
        ..Default::default()
    });
    let tree = root(vec![other_section, tab_bar], rect(0.0, 0.0, 400.0, 900.0));

    let s = Selector::Text {
        text: Pattern::text("Same"),
        modifiers: Modifiers {
            ancestor: Some(Box::new(role_sel(Role::TabBar))),
            ..Default::default()
        },
    };
    let all = resolve_selector_all(&tree, &s);
    assert_eq!(
        all.len(),
        2,
        "ancestor=TabBar should drop stray_same (under other_section), resolve_all should return 2 (tab_a + tab_b); without the filter it returns 3, actual {}",
        all.len()
    );
}

#[test]
fn ancestor_nested_selector_two_levels() {
    // tree:
    //   root (app)
    //   ├── stray_leaf (label="Leaf") — DFS first, not under the nested ancestor chain
    //   └── c_node (role=NavigationBar)
    //       └── b_node (role=TabBar)
    //           └── a_leaf (label="Leaf")
    // selector A = text("Leaf") + ancestor=B;  B = role(TabBar) + ancestor=C;  C = role(NavigationBar)
    // → with nested ancestors firing, hit a_leaf (bounds.y=300); without, hit stray_leaf (bounds.y=50).
    let stray_leaf = mk(NodePartial {
        raw_type: Some("staticText".into()),
        label: Some("Leaf".into()),
        bounds: Some(rect(200.0, 50.0, 80.0, 30.0)),
        ..Default::default()
    });
    let a_leaf = mk(NodePartial {
        raw_type: Some("staticText".into()),
        label: Some("Leaf".into()),
        bounds: Some(rect(50.0, 300.0, 80.0, 30.0)),
        ..Default::default()
    });
    let b_node = mk(NodePartial {
        raw_type: Some("tabBar".into()),
        role: Some(Role::TabBar),
        bounds: Some(rect(0.0, 200.0, 400.0, 200.0)),
        children: Some(vec![a_leaf]),
        ..Default::default()
    });
    let c_node = mk(NodePartial {
        raw_type: Some("navigationBar".into()),
        role: Some(Role::NavigationBar),
        bounds: Some(rect(0.0, 100.0, 400.0, 400.0)),
        children: Some(vec![b_node]),
        ..Default::default()
    });
    let tree = root(vec![stray_leaf, c_node], rect(0.0, 0.0, 400.0, 900.0));

    let nested_b = Selector::Role {
        role: Role::TabBar,
        name: None,
        modifiers: Modifiers {
            ancestor: Some(Box::new(role_sel(Role::NavigationBar))),
            ..Default::default()
        },
    };
    let s = Selector::Text {
        text: Pattern::text("Leaf"),
        modifiers: Modifiers {
            ancestor: Some(Box::new(nested_b)),
            ..Default::default()
        },
    };
    let picked = resolve_selector(&tree, &s).expect("should hit a_leaf");
    assert_eq!(
        picked.bounds.y, 300.0,
        "2-level nested ancestor (A.ancestor=B; B.ancestor=C) should hit a_leaf (y=300); without the filter, hits stray_leaf (y=50), actual y={}",
        picked.bounds.y
    );
}

#[test]
fn ancestor_picks_non_tappable_when_tappable_sibling_lacks_ancestor() {
    // Pipeline ordering: ancestor MUST run before
    // tappable_subset_filter. Otherwise, when candidates mix a tappable
    // node outside the ancestor chain and a non-tappable node inside,
    // tappable filter drops the non-tappable first → ancestor filter
    // then drops the remaining tappable → empty result.
    //
    // Real-sim scenario: sub-tab "Alerts" (rt='other', under a
    // horizontal scrollView sub-tab nav) vs bottom-tab "Alerts"
    // (rt='button', under UITabBar). Selector text("Alerts") +
    // ancestor=role(ScrollView) targets the sub-tab. Ancestor-first
    // ordering must yield the sub-tab.
    //
    // tree:
    //   root
    //   ├── tab_bar (TabBar)
    //   │   └── bottom_tab_alerts (rt='button', label='Alerts', tappable, NO ScrollView ancestor)
    //   └── scroll_view (ScrollView)
    //       └── sub_tab_alerts (rt='other', label='Alerts', non-tappable, ScrollView ancestor ✓)
    let bottom_tab_alerts = mk(NodePartial {
        raw_type: Some("button".into()),
        label: Some("Alerts".into()),
        bounds: Some(rect(231.0, 795.0, 77.0, 54.0)),
        ..Default::default()
    });
    let tab_bar = mk(NodePartial {
        raw_type: Some("tabBar".into()),
        role: Some(Role::TabBar),
        bounds: Some(rect(0.0, 780.0, 400.0, 80.0)),
        children: Some(vec![bottom_tab_alerts]),
        ..Default::default()
    });
    let sub_tab_alerts = mk(NodePartial {
        raw_type: Some("other".into()),
        label: Some("Alerts".into()),
        bounds: Some(rect(50.0, 61.0, 62.0, 56.0)),
        ..Default::default()
    });
    let scroll_view = mk(NodePartial {
        raw_type: Some("scrollView".into()),
        role: Some(Role::ScrollView),
        bounds: Some(rect(0.0, 50.0, 400.0, 80.0)),
        children: Some(vec![sub_tab_alerts]),
        ..Default::default()
    });
    let tree = root(vec![tab_bar, scroll_view], rect(0.0, 0.0, 400.0, 900.0));

    let s = Selector::Text {
        text: Pattern::text("Alerts"),
        modifiers: Modifiers {
            ancestor: Some(Box::new(role_sel(Role::ScrollView))),
            ..Default::default()
        },
    };
    let picked = resolve_selector(&tree, &s)
        .expect("ancestor must run before tappable; should resolve sub-tab Alerts");
    assert_eq!(
        picked.bounds.y, 61.0,
        "should hit sub-tab Alerts (y=61, rt=other, non-tappable, scrollView ancestor); if the tappable filter runs first it gets dropped → ancestor then drops the button → empty, actual y={}",
        picked.bounds.y
    );
}

#[test]
fn ancestor_filter_runs_before_spatial_pipeline() {
    // ancestor + below combined modifiers, end-to-end final-candidate check.
    // tree:
    //   root (app)
    //   ├── nav_bar (role=TabBar)
    //   │   ├── header (label="Header", bounds=(0,460,500,20))
    //   │   ├── match_below_in_navbar (tab, label="Match", bounds=(0,490,80,30))  ← below Header
    //   │   └── match_above_in_navbar (tab, label="Match", bounds=(0,420,80,30))  ← above Header
    //   └── other_subtree
    //       └── match_below_outside_navbar (tab, label="Match", bounds=(200,490,80,30))
    // selector = text("Match") + ancestor=role(TabBar) + below=text("Header")
    // → match_below_in_navbar.
    let header = mk(NodePartial {
        raw_type: Some("staticText".into()),
        label: Some("Header".into()),
        bounds: Some(rect(0.0, 460.0, 500.0, 20.0)),
        ..Default::default()
    });
    let match_below_in_navbar = mk(NodePartial {
        raw_type: Some("tab".into()),
        label: Some("Match".into()),
        bounds: Some(rect(0.0, 490.0, 80.0, 30.0)),
        ..Default::default()
    });
    let match_above_in_navbar = mk(NodePartial {
        raw_type: Some("tab".into()),
        label: Some("Match".into()),
        bounds: Some(rect(0.0, 420.0, 80.0, 30.0)),
        ..Default::default()
    });
    let nav_bar = mk(NodePartial {
        raw_type: Some("tabBar".into()),
        role: Some(Role::TabBar),
        bounds: Some(rect(0.0, 400.0, 500.0, 200.0)),
        children: Some(vec![header, match_below_in_navbar, match_above_in_navbar]),
        ..Default::default()
    });
    let match_below_outside_navbar = mk(NodePartial {
        raw_type: Some("tab".into()),
        label: Some("Match".into()),
        bounds: Some(rect(200.0, 490.0, 80.0, 30.0)),
        ..Default::default()
    });
    let other_subtree = mk(NodePartial {
        raw_type: Some("other".into()),
        bounds: Some(rect(0.0, 0.0, 500.0, 400.0)),
        children: Some(vec![match_below_outside_navbar]),
        ..Default::default()
    });
    // other_subtree comes before nav_bar so DFS reaches
    // match_below_outside_navbar first. Without the ancestor filter,
    // spatial "below" drops match_above_in_navbar, leaving two
    // candidates; DFS first = match_below_outside_navbar (bounds.x=200).
    // With the ancestor filter, only match_below_in_navbar (x=0) remains.
    let tree = root(vec![other_subtree, nav_bar], rect(0.0, 0.0, 500.0, 900.0));

    let s = Selector::Text {
        text: Pattern::text("Match"),
        modifiers: Modifiers {
            ancestor: Some(Box::new(role_sel(Role::TabBar))),
            below: Some(Box::new(text_sel("Header"))),
            ..Default::default()
        },
    };
    let picked =
        resolve_selector(&tree, &s).expect("combined modifiers should hit match_below_in_navbar");
    assert_eq!(
        (picked.bounds.x, picked.bounds.y),
        (0.0, 490.0),
        "should hit match_below_in_navbar (ancestor chain contains TabBar + geometrically below Header); without the ancestor filter, hits match_below_outside_navbar (x=200), actual bounds=({},{})",
        picked.bounds.x,
        picked.bounds.y
    );
}

// ---- index pick (first / last / nth) ------------------------------------

#[test]
fn index_nth_picks_zero_based() {
    let n1 = mk(NodePartial {
        label: Some("X".into()),
        bounds: Some(rect(0.0, 0.0, 10.0, 10.0)),
        ..Default::default()
    });
    let n2 = mk(NodePartial {
        label: Some("X".into()),
        bounds: Some(rect(20.0, 20.0, 10.0, 10.0)),
        ..Default::default()
    });
    let n3 = mk(NodePartial {
        label: Some("X".into()),
        bounds: Some(rect(40.0, 40.0, 10.0, 10.0)),
        ..Default::default()
    });
    let tree = root(vec![n1, n2, n3], rect(0.0, 0.0, 500.0, 500.0));
    let s = Selector::Text {
        text: Pattern::text("X"),
        modifiers: Modifiers {
            nth: Some(1),
            ..Default::default()
        },
    };
    let got = resolve_selector(&tree, &s).expect("hit");
    assert_eq!(got.bounds, rect(20.0, 20.0, 10.0, 10.0));
}

#[test]
fn index_nth_out_of_range_returns_none() {
    let n = mk(NodePartial {
        label: Some("X".into()),
        bounds: Some(rect(0.0, 0.0, 10.0, 10.0)),
        ..Default::default()
    });
    let tree = root(vec![n], rect(0.0, 0.0, 500.0, 500.0));
    let s = Selector::Text {
        text: Pattern::text("X"),
        modifiers: Modifiers {
            nth: Some(5),
            ..Default::default()
        },
    };
    assert!(resolve_selector(&tree, &s).is_none());
}

#[test]
fn index_first_picks_dfs_first() {
    let n1 = mk(NodePartial {
        label: Some("X".into()),
        bounds: Some(rect(0.0, 0.0, 10.0, 10.0)),
        ..Default::default()
    });
    let n2 = mk(NodePartial {
        label: Some("X".into()),
        bounds: Some(rect(20.0, 20.0, 10.0, 10.0)),
        ..Default::default()
    });
    let tree = root(vec![n1, n2], rect(0.0, 0.0, 500.0, 500.0));
    let s = Selector::Text {
        text: Pattern::text("X"),
        modifiers: Modifiers {
            first: Some(true),
            ..Default::default()
        },
    };
    let got = resolve_selector(&tree, &s).expect("hit");
    assert_eq!(got.bounds, rect(0.0, 0.0, 10.0, 10.0));
}

#[test]
fn index_last_picks_dfs_last() {
    let n1 = mk(NodePartial {
        label: Some("X".into()),
        bounds: Some(rect(0.0, 0.0, 10.0, 10.0)),
        ..Default::default()
    });
    let n2 = mk(NodePartial {
        label: Some("X".into()),
        bounds: Some(rect(20.0, 20.0, 10.0, 10.0)),
        ..Default::default()
    });
    let tree = root(vec![n1, n2], rect(0.0, 0.0, 500.0, 500.0));
    let s = Selector::Text {
        text: Pattern::text("X"),
        modifiers: Modifiers {
            last: Some(true),
            ..Default::default()
        },
    };
    let got = resolve_selector(&tree, &s).expect("hit");
    assert_eq!(got.bounds, rect(20.0, 20.0, 10.0, 10.0));
}

// ---- anchor-only base form (cases F-K) ---------------------------------

#[test]
fn anchor_below_picks_dfs_first_candidate_below() {
    let anchor = mk(NodePartial {
        label: Some("Anchor".into()),
        bounds: Some(rect(0.0, 0.0, 10.0, 10.0)),
        ..Default::default()
    });
    let cand1 = mk(NodePartial {
        label: Some("C1".into()),
        bounds: Some(rect(0.0, 95.0, 10.0, 10.0)),
        ..Default::default()
    });
    let cand2 = mk(NodePartial {
        label: Some("C2".into()),
        bounds: Some(rect(0.0, 195.0, 10.0, 10.0)),
        ..Default::default()
    });
    let tree = root(
        vec![anchor, cand1, cand2],
        rect(-1000.0, -1000.0, 2000.0, 2000.0),
    );
    let s = Selector::Anchor {
        anchor: AnchorBox {
            below: Some(Box::new(text_sel("Anchor"))),
            ..Default::default()
        },
        index: IndexModifiers::default(),
    };
    let got = resolve_selector(&tree, &s).expect("hit");
    assert_eq!(got.label.as_deref(), Some("C1"));
}

#[test]
fn anchor_below_all_returns_all_dfs_order() {
    let anchor = mk(NodePartial {
        label: Some("Anchor".into()),
        bounds: Some(rect(0.0, 0.0, 10.0, 10.0)),
        ..Default::default()
    });
    let cand1 = mk(NodePartial {
        label: Some("C1".into()),
        bounds: Some(rect(0.0, 95.0, 10.0, 10.0)),
        ..Default::default()
    });
    let cand2 = mk(NodePartial {
        label: Some("C2".into()),
        bounds: Some(rect(0.0, 195.0, 10.0, 10.0)),
        ..Default::default()
    });
    let tree = root(
        vec![anchor, cand1, cand2],
        rect(-1000.0, -1000.0, 2000.0, 2000.0),
    );
    let s = Selector::Anchor {
        anchor: AnchorBox {
            below: Some(Box::new(text_sel("Anchor"))),
            ..Default::default()
        },
        index: IndexModifiers::default(),
    };
    let all = resolve_selector_all(&tree, &s);
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].label.as_deref(), Some("C1"));
    assert_eq!(all[1].label.as_deref(), Some("C2"));
}

#[test]
fn anchor_below_nth_picks_second() {
    let anchor = mk(NodePartial {
        label: Some("Anchor".into()),
        bounds: Some(rect(0.0, 0.0, 10.0, 10.0)),
        ..Default::default()
    });
    let cand1 = mk(NodePartial {
        label: Some("C1".into()),
        bounds: Some(rect(0.0, 95.0, 10.0, 10.0)),
        ..Default::default()
    });
    let cand2 = mk(NodePartial {
        label: Some("C2".into()),
        bounds: Some(rect(0.0, 195.0, 10.0, 10.0)),
        ..Default::default()
    });
    let tree = root(
        vec![anchor, cand1, cand2],
        rect(-1000.0, -1000.0, 2000.0, 2000.0),
    );
    let s = Selector::Anchor {
        anchor: AnchorBox {
            below: Some(Box::new(text_sel("Anchor"))),
            ..Default::default()
        },
        index: IndexModifiers {
            nth: Some(1),
            ..Default::default()
        },
    };
    let got = resolve_selector(&tree, &s).expect("hit");
    assert_eq!(got.label.as_deref(), Some("C2"));
}

#[test]
fn anchor_miss_short_circuits_to_none() {
    let n = mk(NodePartial {
        label: Some("X".into()),
        ..Default::default()
    });
    let tree = root(vec![n], rect(0.0, 0.0, 500.0, 500.0));
    let s = Selector::Anchor {
        anchor: AnchorBox {
            below: Some(Box::new(text_sel("NoSuch"))),
            ..Default::default()
        },
        index: IndexModifiers::default(),
    };
    assert!(resolve_selector(&tree, &s).is_none());
}

#[test]
fn anchor_left_of_and_above_both_apply() {
    // case K — leftOf AND above intersection.
    let center = mk(NodePartial {
        label: Some("Center".into()),
        bounds: Some(rect(45.0, 45.0, 10.0, 10.0)),
        ..Default::default()
    });
    let bottom = mk(NodePartial {
        label: Some("Bottom".into()),
        bounds: Some(rect(45.0, 195.0, 10.0, 10.0)),
        ..Default::default()
    });
    let matches_n = mk(NodePartial {
        label: Some("matches".into()),
        bounds: Some(rect(0.0, 0.0, 10.0, 10.0)),
        ..Default::default()
    });
    let right_of_center = mk(NodePartial {
        label: Some("rightOfCenter".into()),
        bounds: Some(rect(200.0, 0.0, 10.0, 10.0)),
        ..Default::default()
    });
    let tree = root(
        vec![center, bottom, matches_n, right_of_center],
        rect(-1000.0, -1000.0, 2000.0, 2000.0),
    );
    let s = Selector::Anchor {
        anchor: AnchorBox {
            left_of: Some(Box::new(text_sel("Center"))),
            above: Some(Box::new(text_sel("Bottom"))),
            ..Default::default()
        },
        index: IndexModifiers::default(),
    };
    let all = resolve_selector_all(&tree, &s);
    let labels: Vec<&str> = all.iter().filter_map(|n| n.label.as_deref()).collect();
    assert!(labels.contains(&"matches"));
    assert!(!labels.contains(&"rightOfCenter"));
}

// ---- topmost hit-test (modal overlay) -----------------------------------
// Apple `XCUIElementQuery.firstMatch` + maestro `findElement` semantic:
// when ≥1 candidate is inside a Role::Alert / Role::Dialog subtree, the
// resolver only returns those modal-subtree candidates (drops underlying
// drawer/page candidates). When no modal overlay is present, fall back to
// the existing DFS pre-order so prior behaviour is preserved.

#[test]
fn topmost_hit_test_picks_modal_button_when_same_label_underneath() {
    let drawer_logout = mk(NodePartial {
        role: Some(Role::Button),
        label: Some("Log out".into()),
        bounds: Some(rect(50.0, 778.0, 100.0, 40.0)),
        ..Default::default()
    });
    let alert_logout = mk(NodePartial {
        role: Some(Role::Button),
        label: Some("Log out".into()),
        bounds: Some(rect(150.0, 463.0, 80.0, 30.0)),
        ..Default::default()
    });
    let alert_cancel = mk(NodePartial {
        role: Some(Role::Button),
        label: Some("Cancel".into()),
        bounds: Some(rect(40.0, 463.0, 80.0, 30.0)),
        ..Default::default()
    });
    let alert = mk(NodePartial {
        role: Some(Role::Alert),
        bounds: Some(rect(40.0, 425.0, 320.0, 200.0)),
        children: Some(vec![alert_cancel, alert_logout]),
        ..Default::default()
    });
    // Pre-order: drawer_logout FIRST, then alert — so DFS pre-order naturally
    // picks the drawer button (wrong), and topmost filter must drop drawer
    // and pick the modal-subtree button (right).
    let tree = root(vec![drawer_logout, alert], rect(0.0, 0.0, 390.0, 844.0));
    let got = resolve_selector(&tree, &text_sel("Log out"));
    assert!(got.is_some(), "expected modal alert button to be picked");
    let b = &got.unwrap().bounds;
    let cy = b.y + b.h / 2.0;
    assert!(
        cy < 600.0,
        "expected the alert button (y≈478), got bounds={:?}",
        b
    );
}

#[test]
fn topmost_hit_test_no_modal_falls_back_to_dfs_first() {
    let first_logout = mk(NodePartial {
        role: Some(Role::Button),
        label: Some("Log out".into()),
        bounds: Some(rect(50.0, 200.0, 100.0, 40.0)),
        ..Default::default()
    });
    let second_logout = mk(NodePartial {
        role: Some(Role::Button),
        label: Some("Log out".into()),
        bounds: Some(rect(50.0, 600.0, 100.0, 40.0)),
        ..Default::default()
    });
    // No Alert / Dialog in the tree — both candidates are "under no modal".
    let tree = root(
        vec![first_logout, second_logout],
        rect(0.0, 0.0, 390.0, 844.0),
    );
    let got = resolve_selector(&tree, &text_sel("Log out"));
    assert!(got.is_some());
    let b = &got.unwrap().bounds;
    let cy = b.y + b.h / 2.0;
    // DFS pre-order picks the first one (y=200 → cy=220).
    assert!(
        cy < 400.0,
        "expected first DFS candidate (y≈220), got bounds={:?}",
        b
    );
}

#[test]
fn topmost_hit_test_modal_with_disjoint_button_still_picks_modal_descendant() {
    let drawer_submit = mk(NodePartial {
        role: Some(Role::Button),
        label: Some("Submit".into()),
        bounds: Some(rect(50.0, 778.0, 100.0, 40.0)),
        ..Default::default()
    });
    let dialog_submit = mk(NodePartial {
        role: Some(Role::Button),
        label: Some("Submit".into()),
        bounds: Some(rect(150.0, 463.0, 80.0, 30.0)),
        ..Default::default()
    });
    let dialog_cancel = mk(NodePartial {
        role: Some(Role::Button),
        label: Some("Cancel".into()),
        bounds: Some(rect(40.0, 463.0, 80.0, 30.0)),
        ..Default::default()
    });
    let dialog = mk(NodePartial {
        role: Some(Role::Dialog),
        bounds: Some(rect(40.0, 425.0, 320.0, 200.0)),
        children: Some(vec![dialog_cancel, dialog_submit]),
        ..Default::default()
    });
    let tree = root(vec![drawer_submit, dialog], rect(0.0, 0.0, 390.0, 844.0));
    // Drawer Submit appears first in DFS pre-order, but it's outside the
    // Dialog subtree — topmost filter must drop it.
    let got = resolve_selector(&tree, &text_sel("Submit"));
    assert!(got.is_some());
    let b = &got.unwrap().bounds;
    let cy = b.y + b.h / 2.0;
    assert!(
        cy < 600.0,
        "expected dialog Submit (y≈478), got bounds={:?}",
        b
    );
}

#[test]
fn topmost_hit_test_detects_modal_by_raw_type_when_role_absent() {
    // Real-sim payloads carry raw_type="alert" but role=None (the Swift
    // /tree route only emits rawType, no role field). The filter must
    // detect the modal via raw_type fall-through.
    let drawer_logout = mk(NodePartial {
        role: Some(Role::Button),
        label: Some("Log out".into()),
        bounds: Some(rect(50.0, 778.0, 100.0, 40.0)),
        ..Default::default()
    });
    let alert_logout = mk(NodePartial {
        // role intentionally None — wire shape from Swift.
        raw_type: Some("button".into()),
        label: Some("Log out".into()),
        bounds: Some(rect(150.0, 463.0, 80.0, 30.0)),
        ..Default::default()
    });
    let alert = mk(NodePartial {
        raw_type: Some("alert".into()), // <-- the only modal marker on the wire
        bounds: Some(rect(40.0, 425.0, 320.0, 200.0)),
        children: Some(vec![alert_logout]),
        ..Default::default()
    });
    let tree = root(vec![drawer_logout, alert], rect(0.0, 0.0, 390.0, 844.0));
    let got = resolve_selector(&tree, &text_sel("Log out"));
    assert!(got.is_some(), "expected modal alert button");
    let b = &got.unwrap().bounds;
    let cy = b.y + b.h / 2.0;
    assert!(
        cy < 600.0,
        "expected the alert button (y≈478), got bounds={:?}",
        b
    );
}

#[test]
fn tappable_filter_drops_alert_container_and_static_text_in_modal() {
    // Real app logout modal shape:
    // - drawer button [Log out]
    // - alert (raw_type="alert", label="Log out")
    //     staticText [Log out]   (title)
    //     staticText [Are you sure ...]
    //       button [Cancel]
    //       button [Log out]
    // Single `text("Log out")` selector must resolve to the alert button,
    // not the alert container (rawType=alert, label="Log out") nor the
    // staticText title (DFS-first inside the modal).
    let drawer_button = mk(NodePartial {
        raw_type: Some("button".into()),
        label: Some("Log out".into()),
        bounds: Some(rect(150.0, 779.0, 235.0, 44.0)),
        ..Default::default()
    });
    let title_static = mk(NodePartial {
        raw_type: Some("staticText".into()),
        label: Some("Log out".into()),
        bounds: Some(rect(71.0, 397.0, 260.0, 20.0)),
        ..Default::default()
    });
    let confirm_static = mk(NodePartial {
        raw_type: Some("staticText".into()),
        label: Some("Are you sure you want to log out?".into()),
        bounds: Some(rect(71.0, 425.0, 260.0, 18.0)),
        ..Default::default()
    });
    let cancel_btn = mk(NodePartial {
        raw_type: Some("button".into()),
        label: Some("Cancel".into()),
        bounds: Some(rect(57.0, 463.0, 140.0, 48.0)),
        ..Default::default()
    });
    let alert_logout_btn = mk(NodePartial {
        raw_type: Some("button".into()),
        label: Some("Log out".into()),
        bounds: Some(rect(205.0, 463.0, 140.0, 48.0)),
        ..Default::default()
    });
    let confirm_with_buttons = mk(NodePartial {
        raw_type: Some("staticText".into()),
        label: Some("Are you sure you want to log out?".into()),
        bounds: Some(rect(71.0, 425.0, 260.0, 18.0)),
        children: Some(vec![cancel_btn, alert_logout_btn]),
        ..Default::default()
    });
    let _ = confirm_static;
    let alert = mk(NodePartial {
        raw_type: Some("alert".into()),
        label: Some("Log out".into()),
        bounds: Some(rect(41.0, 375.0, 320.0, 152.0)),
        children: Some(vec![title_static, confirm_with_buttons]),
        ..Default::default()
    });
    let tree = root(vec![drawer_button, alert], rect(0.0, 0.0, 402.0, 874.0));
    let got = resolve_selector(&tree, &text_sel("Log out"));
    assert!(got.is_some(), "expected alert button to be picked");
    let n = got.unwrap();
    assert_eq!(n.raw_type, "button");
    let b = &n.bounds;
    // alert button center: (205+70, 463+24) = (275, 487)
    assert!(
        b.x > 200.0 && b.y > 460.0 && b.y < 520.0,
        "expected alert button bounds, got {:?}",
        b
    );
}

#[test]
fn tappable_filter_keeps_static_text_when_no_button_candidate() {
    // Non-modal text-on-staticText scenario — single staticText match,
    // no tappable around. Filter must keep the staticText.
    let n = mk(NodePartial {
        raw_type: Some("staticText".into()),
        label: Some("Heading".into()),
        bounds: Some(rect(20.0, 100.0, 200.0, 24.0)),
        ..Default::default()
    });
    let tree = root(vec![n], rect(0.0, 0.0, 402.0, 874.0));
    let got = resolve_selector(&tree, &text_sel("Heading"));
    assert!(got.is_some());
    assert_eq!(got.unwrap().raw_type, "staticText");
}

#[test]
fn topmost_hit_test_first_modifier_still_respects_dfs_when_no_modal() {
    let n1 = mk(NodePartial {
        role: Some(Role::Button),
        label: Some("X".into()),
        bounds: Some(rect(50.0, 200.0, 100.0, 40.0)),
        ..Default::default()
    });
    let n2 = mk(NodePartial {
        role: Some(Role::Button),
        label: Some("X".into()),
        bounds: Some(rect(50.0, 600.0, 100.0, 40.0)),
        ..Default::default()
    });
    // No modal — the first: Some(true) modifier must keep DFS pre-order
    // semantic (regression cover for index pick).
    let tree = root(vec![n1, n2], rect(0.0, 0.0, 390.0, 844.0));
    let s = Selector::Text {
        text: Pattern::text("X"),
        modifiers: Modifiers {
            first: Some(true),
            ..Default::default()
        },
    };
    let got = resolve_selector(&tree, &s);
    assert!(got.is_some());
    let b = &got.unwrap().bounds;
    let cy = b.y + b.h / 2.0;
    assert!(cy < 400.0, "expected first DFS (y≈220), got bounds={:?}", b);
}

#[test]
fn spatial_picks_non_tappable_when_tappable_sibling_fails_filter() {
    // Pipeline ordering: the spatial filter MUST run before
    // tappable_subset_filter. Same root cause as the ancestor
    // ordering fix — explicit structural intent (spatial modifier
    // given by the user) overrides the implicit interactive preference
    // (tappable filter). When a non-tappable candidate is the one
    // that satisfies the spatial constraint and a tappable sibling
    // sits in a position that fails the spatial constraint,
    // tappable-first would drop the non-tappable, leaving only the
    // spatially-failing tappable → empty.
    //
    // Real-sim scenario: a horizontal sub-tab "Tracking" (rt='other',
    // y=140, NOT under a scrollView ancestor) and UITabBar bottom-tab
    // "Tracking" (rt='button', y=795). The intended sub-tab uses
    // Selector::Text + Modifiers::above(text("Device")) — the sub-tab
    // Tracking center.y=157 < Device bottom-tab center.y=822 ✓;
    // bottom-tab Tracking center.y=822 = Device center.y=822 ✗
    // (strict above). Spatial-first ordering must yield the sub-tab.
    //
    // tree:
    //   root
    //   ├── tab_bar (TabBar)
    //   │   ├── bottom_tab_device (rt='button', label='Device', y=795)
    //   │   └── bottom_tab_tracking (rt='button', label='Tracking', y=795, tappable, NOT above Device)
    //   └── sub_tab_nav (rt='other')
    //       └── sub_tab_tracking (rt='other', label='Tracking', y=140, non-tappable, above Device ✓)
    let bottom_tab_device = mk(NodePartial {
        raw_type: Some("button".into()),
        label: Some("Device".into()),
        bounds: Some(rect(30.0, 795.0, 77.0, 54.0)),
        ..Default::default()
    });
    let bottom_tab_tracking = mk(NodePartial {
        raw_type: Some("button".into()),
        label: Some("Tracking".into()),
        bounds: Some(rect(300.0, 795.0, 77.0, 54.0)),
        ..Default::default()
    });
    let tab_bar = mk(NodePartial {
        raw_type: Some("tabBar".into()),
        role: Some(Role::TabBar),
        bounds: Some(rect(0.0, 780.0, 400.0, 80.0)),
        children: Some(vec![bottom_tab_device, bottom_tab_tracking]),
        ..Default::default()
    });
    let sub_tab_tracking = mk(NodePartial {
        raw_type: Some("other".into()),
        label: Some("Tracking".into()),
        bounds: Some(rect(260.0, 140.0, 120.0, 34.0)),
        ..Default::default()
    });
    let sub_tab_nav = mk(NodePartial {
        raw_type: Some("other".into()),
        bounds: Some(rect(0.0, 120.0, 400.0, 60.0)),
        children: Some(vec![sub_tab_tracking]),
        ..Default::default()
    });
    let tree = root(vec![tab_bar, sub_tab_nav], rect(0.0, 0.0, 400.0, 900.0));

    let s = Selector::Text {
        text: Pattern::text("Tracking"),
        modifiers: Modifiers {
            above: Some(Box::new(Selector::Text {
                text: Pattern::text("Device"),
                modifiers: Modifiers::default(),
            })),
            ..Default::default()
        },
    };
    let picked = resolve_selector(&tree, &s)
        .expect("spatial must run before tappable; should resolve sub-tab Tracking");
    assert_eq!(
        picked.bounds.y, 140.0,
        "should hit sub-tab Tracking (y=140, rt=other, non-tappable, above Device); if the tappable filter runs first it keeps bottom-tab Tracking (y=795), then spatial drops it → empty, actual y={}",
        picked.bounds.y
    );
}
