//! Where a touch reaches an element that something is drawn over.

use smix_host_coord_resolver::{Covering, Touchable, touch_point};
use smix_screen::{A11yNode, Rect, WindowInfo, WindowKind};

fn r(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect { x, y, w, h }
}

fn node(raw: &str, bounds: Rect, children: Vec<A11yNode>) -> A11yNode {
    A11yNode {
        visible_bounds: None,
        hittable: None,
        window: None,
        unreadable_windows: None,
        raw_type: raw.into(),
        element_type_raw: 0,
        role: None,
        identifier: None,
        label: None,
        title: None,
        placeholder_value: None,
        value: None,
        text: None,
        bounds,
        enabled: true,
        selected: false,
        has_focus: false,
        visible: true,
        children,
    }
}

fn window(
    kind: WindowKind,
    package: &str,
    layer: Option<i32>,
    bounds: Rect,
    kids: Vec<A11yNode>,
) -> A11yNode {
    let mut w = node("window", bounds, kids);
    w.window = Some(WindowInfo {
        package: Some(package.into()),
        kind,
        focused: kind == WindowKind::Application,
        layer,
        touchable: Some(bounds),
    });
    w
}

const SCREEN: Rect = Rect {
    x: 0.0,
    y: 0.0,
    w: 1080.0,
    h: 2400.0,
};

/// An Android screen: the status bar (0..136) over an app drawn edge to
/// edge, with `field` in the app.
fn android(field: A11yNode, bar_layer: Option<i32>, app_layer: Option<i32>) -> A11yNode {
    node(
        "root",
        SCREEN,
        vec![
            window(
                WindowKind::System,
                "com.android.systemui",
                bar_layer,
                r(0.0, 0.0, 1080.0, 136.0),
                vec![],
            ),
            window(
                WindowKind::Application,
                "dev.smix.fixture",
                app_layer,
                SCREEN,
                vec![field],
            ),
        ],
    )
}

fn field_of(tree: &A11yNode) -> &A11yNode {
    &tree.children[1].children[0]
}

fn at(t: Touchable<'_>) -> (f64, f64) {
    match t {
        Touchable::At(x, y) => (x, y),
        Touchable::Covered(c) => panic!("expected a point, got covered by {c:?}"),
    }
}

fn close(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
}

#[test]
fn a_centre_under_the_status_bar_moves_to_the_part_below_it() {
    // Field 44..814 x 60..160: its centre (y 110) is under the bar (0..136).
    // What shows below the bar is y 136..160; its centre is y 148.
    let tree = android(
        node("field", r(44.0, 60.0, 770.0, 100.0), vec![]),
        Some(20),
        Some(2),
    );
    let p = at(touch_point(&tree, field_of(&tree)).expect("resolves"));
    assert!(close(p, (429.0 / 1080.0, 148.0 / 2400.0)), "{p:?}");
}

#[test]
fn a_centre_nothing_is_over_is_the_centre() {
    // Field 44..814 x 100..254 is partly under the bar; its centre (y 177)
    // is not, so the touch goes where it always has.
    let tree = android(
        node("field", r(44.0, 100.0, 770.0, 154.0), vec![]),
        Some(20),
        Some(2),
    );
    let p = at(touch_point(&tree, field_of(&tree)).expect("resolves"));
    assert!(close(p, (429.0 / 1080.0, 177.0 / 2400.0)), "{p:?}");
}

#[test]
fn an_element_wholly_under_a_window_above_it_is_covered_and_named() {
    let tree = android(
        node("field", r(44.0, 20.0, 770.0, 100.0), vec![]),
        Some(20),
        Some(2),
    );
    match touch_point(&tree, field_of(&tree)).expect("resolves") {
        Touchable::Covered(covers) => {
            assert_eq!(covers.len(), 1, "{covers:?}");
            match &covers[0].what {
                Covering::Window(w) => {
                    assert_eq!(w.package.as_deref(), Some("com.android.systemui"));
                }
                other => panic!("covered by the wrong thing: {other:?}"),
            }
        }
        Touchable::At(x, y) => panic!("aimed at ({x}, {y}) under the status bar"),
    }
}

#[test]
fn a_window_below_the_elements_own_covers_nothing() {
    // The same geometry with the bar under the app: the app takes the touch.
    let tree = android(
        node("field", r(44.0, 60.0, 770.0, 100.0), vec![]),
        Some(1),
        Some(2),
    );
    let p = at(touch_point(&tree, field_of(&tree)).expect("resolves"));
    assert!(close(p, (429.0 / 1080.0, 110.0 / 2400.0)), "{p:?}");
}

#[test]
fn without_layers_nothing_is_taken_to_be_on_top() {
    // A reader that does not say the stack: the centre, as before.
    let tree = android(
        node("field", r(44.0, 60.0, 770.0, 100.0), vec![]),
        None,
        None,
    );
    let p = at(touch_point(&tree, field_of(&tree)).expect("resolves"));
    assert!(close(p, (429.0 / 1080.0, 110.0 / 2400.0)), "{p:?}");
}

#[test]
fn the_largest_part_left_is_the_one_aimed_at() {
    // The bar covers 0..136; a second window covers x 200..600 over y
    // 136..400, and with it the field's centre (540, 200). The field
    // 0..1080 x 100..300 is left with two parts either side of it:
    // 0..200 and 600..1080 across 136..300. The wider one is aimed at.
    let mut tree = android(
        node("field", r(0.0, 100.0, 1080.0, 200.0), vec![]),
        Some(20),
        Some(2),
    );
    tree.children.insert(
        1,
        window(
            WindowKind::Other,
            "com.example.bubble",
            Some(21),
            r(200.0, 136.0, 400.0, 264.0),
            vec![],
        ),
    );
    let field = &tree.children[2].children[0];
    let p = at(touch_point(&tree, field).expect("resolves"));
    assert!(close(p, (840.0 / 1080.0, 218.0 / 2400.0)), "{p:?}");
}

#[test]
fn only_the_part_that_shows_is_aimed_at() {
    // A row scrolled half off its list: bounds 900..1300, showing 900..1000.
    // Its centre (1100) is outside what shows.
    let mut row = node("row", r(0.0, 900.0, 1080.0, 400.0), vec![]);
    row.visible_bounds = Some(r(0.0, 900.0, 1080.0, 100.0));
    let tree = android(row, Some(20), Some(2));
    let p = at(touch_point(&tree, field_of(&tree)).expect("resolves"));
    assert!(close(p, (540.0 / 1080.0, 950.0 / 2400.0)), "{p:?}");
}

/// iOS: one app tree, the keyboard an element in it.
fn ios(field: Rect) -> A11yNode {
    let screen = r(0.0, 0.0, 390.0, 844.0);
    let keyboard = node(
        "keyboard",
        r(0.0, 500.0, 390.0, 344.0),
        vec![node("key", r(10.0, 520.0, 30.0, 40.0), vec![])],
    );
    node(
        "application",
        screen,
        vec![node("field", field, vec![]), keyboard],
    )
}

#[test]
fn on_ios_the_keyboard_covers_what_is_under_it() {
    // Field 20..370 x 460..560: centre y 510 is on the keyboard (500..);
    // what shows above it is 460..500, centre 480.
    let tree = ios(r(20.0, 460.0, 350.0, 100.0));
    let p = at(touch_point(&tree, &tree.children[0]).expect("resolves"));
    assert!(close(p, (195.0 / 390.0, 480.0 / 844.0)), "{p:?}");
}

#[test]
fn a_key_is_not_covered_by_its_own_keyboard() {
    let tree = ios(r(20.0, 100.0, 350.0, 100.0));
    let key = &tree.children[1].children[0];
    let p = at(touch_point(&tree, key).expect("resolves"));
    assert!(close(p, (25.0 / 390.0, 540.0 / 844.0)), "{p:?}");
}

#[test]
fn on_ios_an_element_wholly_under_the_keyboard_is_covered() {
    let tree = ios(r(20.0, 600.0, 350.0, 100.0));
    match touch_point(&tree, &tree.children[0]).expect("resolves") {
        Touchable::Covered(covers) => assert!(
            covers.iter().any(|c| c.what == Covering::Keyboard),
            "{covers:?}"
        ),
        Touchable::At(x, y) => panic!("aimed at ({x}, {y}) on the keyboard"),
    }
}

#[test]
fn on_ios_a_status_bar_in_the_tree_covers_what_is_under_it() {
    // Status bar 0..54; field 20..370 x 10..80 has its centre (y 45)
    // under it. What shows below the bar is 54..80, centre 67.
    let screen = r(0.0, 0.0, 390.0, 844.0);
    let tree = node(
        "application",
        screen,
        vec![
            node("field", r(20.0, 10.0, 350.0, 70.0), vec![]),
            node("statusBar", r(0.0, 0.0, 390.0, 54.0), vec![]),
        ],
    );
    let p = at(touch_point(&tree, &tree.children[0]).expect("resolves"));
    assert!(close(p, (195.0 / 390.0, 67.0 / 844.0)), "{p:?}");
}

#[test]
fn the_keyboards_window_covers_only_where_it_takes_touches() {
    // Measured on API 36: with the keyboard up, the input method's window
    // spans 136..2340 — everything below the status bar — and takes
    // touches only where the keys are. A field above the keys is not
    // under the keyboard.
    let mut tree = android(
        node("field", r(0.0, 400.0, 1080.0, 124.0), vec![]),
        Some(20),
        Some(2),
    );
    let mut ime = window(
        WindowKind::InputMethod,
        "com.google.android.inputmethod.latin",
        Some(10),
        r(0.0, 136.0, 1080.0, 2204.0),
        vec![],
    );
    ime.window.as_mut().expect("a window").touchable = Some(r(0.0, 1500.0, 1080.0, 840.0));
    tree.children.insert(1, ime);
    let field = &tree.children[2].children[0];
    let p = at(touch_point(&tree, field).expect("resolves"));
    assert!(close(p, (540.0 / 1080.0, 462.0 / 2400.0)), "{p:?}");
}

#[test]
fn a_window_that_does_not_say_where_it_takes_touches_covers_nothing() {
    let mut tree = android(
        node("field", r(44.0, 60.0, 770.0, 100.0), vec![]),
        Some(20),
        Some(2),
    );
    tree.children[0]
        .window
        .as_mut()
        .expect("a window")
        .touchable = None;
    let p = at(touch_point(&tree, field_of(&tree)).expect("resolves"));
    assert!(close(p, (429.0 / 1080.0, 110.0 / 2400.0)), "{p:?}");
}
