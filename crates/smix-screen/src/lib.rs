#![doc = include_str!("../README.md")]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

//! smix-screen — A11yNode + Rect + Bounds + Role types + visibility
//! primitives (stone).
//!
//! # Scope
//!
//! - Pure types (`Rect`, `Bounds`, `Role`, `A11yNode`) with serde wire
//!   compatibility (camelCase JSON, matching the existing Swift-side
//!   SmixRunnerCore `/tree` route shape).
//! - Pure functions (`is_visible_enough`, `visible_area`) that the
//!   selector resolver consumes. No I/O — protocol parsing and types only.
//!
//! # Visibility semantics
//!
//! - `b.w <= 0 || b.h <= 0` → invisible (zero-bounds early reject)
//! - `root.w <= 0 || root.h <= 0` → conservative pass (unknown root)
//! - Otherwise → any non-empty rectangle intersection with `tree.bounds`
//!
//! Matches swift `TreeRoute.isVisible` (any frame ∩ appFrame intersection)
//! and maestro `ViewHierarchy.kt:40-50` `isVisible(node)`.

#![doc(html_root_url = "https://docs.smix.dev/smix-screen")]

use serde::{Deserialize, Serialize};

/// Logical-points rectangle (origin top-left, +x right, +y down — matches
/// UIKit / XCUITest coordinate space). All fields `f64` because the
/// runner `/tree` route emits floating-point points (sub-pixel scale
/// factors).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    /// Top-left x coordinate in logical points.
    pub x: f64,
    /// Top-left y coordinate in logical points.
    pub y: f64,
    /// Width in logical points (zero or negative = invisible).
    pub w: f64,
    /// Height in logical points (zero or negative = invisible).
    pub h: f64,
}

/// Bounds alias — `Bounds` and `Rect` are used interchangeably.
/// Downstream code may prefer one name or the other.
pub type Bounds = Rect;

/// Element summary — projected view of an [`A11yNode`] used in
/// AI-readable failure prompts and `driver.describe()` output.
///
/// `role` is `Some(Role)` for known XCUIElement types, `None` otherwise
/// (serde serializes `None` as null and omits via `skip_serializing_if`;
/// readers must accept both shapes).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementSummary {
    /// Semantic role (None when the underlying XCUIElement type doesn't map).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    /// Primary display name (label → title → text → value → placeholder).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Accessibility identifier (`node.identifier`), if present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Visible text, only when distinct from `name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Geometric bounds in logical points.
    pub bounds: Rect,
    /// Whether the element is currently enabled (interactable).
    pub enabled: bool,
}

/// Aggregate screen description. `elements` is a DFS-collected ordered
/// list of the visible+enabled [`ElementSummary`] entries a reader could
/// name (anonymous layout containers are skipped); `screenshot` is an
/// optional base64 PNG.
///
/// `frontApp` and `capturedAt` are filled at capture. Only `summary` is
/// caller-populated. This comment used to call all three of them that,
/// which is how two fields stayed unconditionally empty while a CLI
/// help string promised a title and a status bar.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenDescription {
    /// Optional base64-encoded PNG screenshot of the screen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<String>,
    /// Nameable visible+enabled elements in DFS pre-order.
    pub elements: Vec<ElementSummary>,
    /// Bundle id of the app this description was taken from, read from
    /// the a11y tree root's identifier.
    ///
    /// Deliberately not called "frontmost": the runner reports the app
    /// it resolved for the request, which is what a caller can act on.
    /// `None` when the root carried no identifier — distinct from an
    /// empty string, which would claim knowledge of an empty answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub front_app: Option<String>,
    /// Free-form one-line summary. The caller writes this; nothing in
    /// smix produces it.
    pub summary: String,
    /// Wall-clock capture timestamp (Unix epoch milliseconds).
    pub captured_at: f64,
}

/// Collect up to `limit` visible+enabled nodes a reader could name (DFS
/// pre-order), projecting each via [`summarize_node`]. Default limit = 1000.
///
/// Anonymous layout scaffolding is skipped — see [`has_identity`].
#[must_use]
pub fn collect_visible_summaries(tree: &A11yNode, limit: usize) -> Vec<ElementSummary> {
    let mut out: Vec<ElementSummary> = Vec::new();
    fn walk(n: &A11yNode, limit: usize, out: &mut Vec<ElementSummary>) {
        if out.len() >= limit {
            return;
        }
        if n.enabled && n.visible {
            let s = summarize_node(n);
            if has_identity(&s) {
                out.push(s);
            }
        }
        for c in &n.children {
            if out.len() >= limit {
                return;
            }
            walk(c, limit, out);
        }
    }
    walk(tree, limit, &mut out);
    out
}

/// Whether a summary tells a reader anything.
///
/// A node with no role, name, id, or text renders as the bare word "unknown"
/// — it cannot be recognized or acted on. Real app trees are mostly these:
/// on a stock Settings screen, 120 of 166 visible nodes are anonymous
/// layout scaffolding, and they sit at the top in DFS order. A failure
/// showing its ten "visible elements" would spend nine of them on
/// `Application → Window → other → other …` and tell the reader nothing,
/// which defeats the point of putting the screen in the failure at all.
fn has_identity(s: &ElementSummary) -> bool {
    s.role.is_some() || s.name.is_some() || s.id.is_some() || s.text.is_some()
}

/// Default visible-summary limit.
pub const DEFAULT_VISIBLE_LIMIT: usize = 1000;

/// Project an [`A11yNode`] to an [`ElementSummary`].
///
/// `name` priority scan: label → title → text → value → placeholderValue.
/// `text` is only set when distinct from `name`.
#[must_use]
pub fn summarize_node(node: &A11yNode) -> ElementSummary {
    let name = node
        .label
        .clone()
        .or_else(|| node.title.clone())
        .or_else(|| node.text.clone())
        .or_else(|| node.value.clone())
        .or_else(|| node.placeholder_value.clone());
    let text = match (&node.text, &name) {
        (Some(t), Some(n)) if t != n => Some(t.clone()),
        _ => None,
    };
    ElementSummary {
        role: node.role,
        name,
        id: node.identifier.clone(),
        text,
        bounds: node.bounds,
        enabled: node.enabled,
    }
}

/// Accessibility role enum (29 variants).
///
/// `serde(rename_all = "camelCase")` keeps the JSON wire identical to the
/// Swift-side `/tree` route output ("staticText" not "static_text").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    /// Tappable button (UIButton / SwiftUI Button).
    Button,
    /// Hyperlink (anchor-like target).
    Link,
    /// Plain editable text input (UITextField / TextInput).
    TextField,
    /// Password / sensitive text input (input masked).
    SecureTextField,
    /// Search-style text input (UISearchBar).
    SearchField,
    /// On/off toggle (UISwitch).
    Switch,
    /// Generic toggle button (two-state).
    Toggle,
    /// Multi-state checkbox.
    CheckBox,
    /// Radio button (one-of-many select).
    Radio,
    /// Image element (UIImageView).
    Image,
    /// Read-only display label (UILabel).
    StaticText,
    /// Tab element inside a tab bar.
    Tab,
    /// Tab bar container (UITabBar).
    TabBar,
    /// Top navigation bar (UINavigationBar).
    NavigationBar,
    /// List / collection cell (UITableViewCell / UICollectionViewCell).
    Cell,
    /// System alert popup (UIAlertController .alert style).
    Alert,
    /// Modal dialog (UIAlertController .dialog / custom modal).
    Dialog,
    /// Continuous slider input (UISlider).
    Slider,
    /// Progress indicator (UIProgressView).
    ProgressBar,
    /// Date / wheel-style picker (UIPickerView).
    Picker,
    /// Drop-down or action menu.
    Menu,
    /// Single menu item inside a Menu.
    MenuItem,
    /// Scrollable container (UIScrollView).
    ScrollView,
    /// Segmented control (UISegmentedControl).
    SegmentedControl,
    /// Table view (UITableView).
    Table,
    /// Collection view (UICollectionView).
    CollectionView,
    /// Embedded web view (WKWebView).
    WebView,
    /// On-screen software keyboard.
    Keyboard,
}

impl Role {
    /// camelCase string name matching the wire `roleSchema` enum variants.
    /// Used by error / log / describe_selector renderers that need the
    /// wire form without pulling in serde_json.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Button => "button",
            Role::Link => "link",
            Role::TextField => "textField",
            Role::SecureTextField => "secureTextField",
            Role::SearchField => "searchField",
            Role::Switch => "switch",
            Role::Toggle => "toggle",
            Role::CheckBox => "checkBox",
            Role::Radio => "radio",
            Role::Image => "image",
            Role::StaticText => "staticText",
            Role::Tab => "tab",
            Role::TabBar => "tabBar",
            Role::NavigationBar => "navigationBar",
            Role::Cell => "cell",
            Role::Alert => "alert",
            Role::Dialog => "dialog",
            Role::Slider => "slider",
            Role::ProgressBar => "progressBar",
            Role::Picker => "picker",
            Role::Menu => "menu",
            Role::MenuItem => "menuItem",
            Role::ScrollView => "scrollView",
            Role::SegmentedControl => "segmentedControl",
            Role::Table => "table",
            Role::CollectionView => "collectionView",
            Role::WebView => "webView",
            Role::Keyboard => "keyboard",
        }
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Derive the curated [`Role`] from the raw XCUIElement type name (e.g.
/// `"button"`, `"radioButton"`, `"progressIndicator"`).
///
/// The Swift `/tree` route only emits `rawType` on the wire — it never
/// fills `role` — so the Rust-side `A11yNode.role` is `None` for every
/// real-sim payload. This function gives the sense layer a single
/// canonical place to lift `rawType` strings into semantic [`Role`]
/// values, matching the reverse direction of the swift
/// `TreeRoute.elementTypeName(_:)` table.
///
/// Returns `None` when the raw type has no curated semantic (`"any"`,
/// `"other"`, `"window"`, `"group"`, …) — Selector::Role still won't
/// match those, which is the intended behaviour.
#[must_use]
pub fn role_from_raw_type(raw_type: &str) -> Option<Role> {
    Some(match raw_type {
        "button" => Role::Button,
        "link" => Role::Link,
        "textField" => Role::TextField,
        "secureTextField" => Role::SecureTextField,
        "searchField" => Role::SearchField,
        "switch" => Role::Switch,
        "toggle" => Role::Toggle,
        "checkBox" => Role::CheckBox,
        // Swift wire uses "radioButton"; Rust enum uses Radio.
        "radioButton" => Role::Radio,
        "image" => Role::Image,
        "staticText" => Role::StaticText,
        "tabBar" => Role::TabBar,
        "navigationBar" => Role::NavigationBar,
        "cell" => Role::Cell,
        "alert" => Role::Alert,
        "dialog" => Role::Dialog,
        "slider" => Role::Slider,
        // Swift wire uses "progressIndicator"; Rust enum uses ProgressBar.
        "progressIndicator" => Role::ProgressBar,
        "picker" => Role::Picker,
        "menu" => Role::Menu,
        "menuItem" => Role::MenuItem,
        "scrollView" => Role::ScrollView,
        "segmentedControl" => Role::SegmentedControl,
        "table" => Role::Table,
        "collectionView" => Role::CollectionView,
        "webView" => Role::WebView,
        "keyboard" => Role::Keyboard,
        // Role::Tab has no corresponding swift elementTypeName case —
        // tabs come through as their containing element type. Leave None.
        _ => return None,
    })
}

/// Recursively fill `node.role` from `node.raw_type` whenever it is
/// currently `None`. Host-set roles (test fixtures, recorder output) are
/// left untouched.
///
/// Call once on the root after a wire deserialize (runner /tree response,
/// recorder snapshot replay, ...) to make `Selector::Role` work against
/// real-sim payloads where the wire only carries `rawType`.
///
/// iOS `UITabBar` items are `button`s nested inside a `tabBar` subtree
/// — there is NO distinct tab `XCUIElement.ElementType` (the swift
/// `elementTypeName` table has no "tab" case), so `rawType` alone can
/// never yield [`Role::Tab`]. A button that lives anywhere inside a
/// `tabBar` subtree is structurally a tab item, so it derives
/// [`Role::Tab`] instead of [`Role::Button`]. The inference is ancestor-
/// based (the real tree nests the tab buttons under wrapper `other`
/// nodes), the only locale-invariant way to make
/// `Selector::Role { Role::Tab }` match real tab-bar items.
pub fn derive_roles_recursive(node: &mut A11yNode) {
    derive_roles_inner(node, false);
}

fn derive_roles_inner(node: &mut A11yNode, inside_tab_bar: bool) {
    if node.role.is_none() {
        node.role = if inside_tab_bar && node.raw_type == "button" {
            Some(Role::Tab)
        } else {
            role_from_raw_type(&node.raw_type)
        };
    }
    let child_inside = inside_tab_bar || node.raw_type == "tabBar";
    for child in &mut node.children {
        derive_roles_inner(child, child_inside);
    }
}

/// Older wire payloads predate the `elementTypeRaw` field; default to
/// 1 (`.other`), which is the safest fallback and matches how Swift
/// `elementTypeName` treats unknown raw values.
fn default_element_type_raw() -> u64 {
    1
}

/// Accessibility tree node — a single-node snapshot as returned by `/tree`,
/// mirroring the Swift-side `TreeRoute.nodeToDict` shape.
///
/// `rawType` carries the underlying Apple `XCUIElement.ElementType` raw
/// name (e.g. "any", "other", "staticText"); `role` is the curated semantic
/// mapping (None when XCUIElement type doesn't map to a known [`Role`]).
///
/// Each optional string field maps to a single Apple a11y attribute, in
/// the order the standard iOS a11y drivers scan them.
///
/// `#[serde(default)]` on the recursive `children: Vec<A11yNode>` allows
/// terminal nodes in JSON to omit the field entirely (`/tree` route emits
/// `"children":[]` consistently but we accept both for forward-compat).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct A11yNode {
    /// Raw Apple `XCUIElement.ElementType` name (e.g. `"any"`, `"other"`).
    pub raw_type: String,
    /// Raw numeric `XCUIElement.ElementType.rawValue`.
    /// `raw_type` above is a string form derived by `elementTypeName`,
    /// but consumers debugging a degraded a11y tree (RN Fabric on iOS
    /// 26.5 is the motivating case) need the numeric form to spot
    /// "iOS types this as .button (9) but identifier / label empty"
    /// — the signature of an app-side accessibility-bridge drop. The
    /// numeric form also disambiguates the alert/dialog button
    /// promotion (rawType is lifted to "button" for consumer
    /// selectors, but the original ElementType number stays here).
    /// Defaults to 1 (`.other`) for wire payloads that omit it.
    #[serde(default = "default_element_type_raw")]
    pub element_type_raw: u64,
    /// Curated semantic role; None when the raw type doesn't map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    /// Accessibility identifier (Apple `accessibilityIdentifier`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    /// Accessibility label (Apple `accessibilityLabel`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Element title (Apple `title`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Placeholder text shown when the field is empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder_value: Option<String>,
    /// Element value (Apple `value`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Visible text content (Apple `text`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Geometric bounds in logical points.
    pub bounds: Rect,
    /// Whether the element is currently interactable.
    pub enabled: bool,
    /// Whether the element is currently selected.
    pub selected: bool,
    /// Whether the element currently has keyboard focus.
    pub has_focus: bool,
    /// Whether Apple's accessibility runtime reports this element as visible.
    pub visible: bool,
    /// Child nodes in stable DFS pre-order.
    #[serde(default)]
    pub children: Vec<A11yNode>,
}

// -------------------- visibility primitives ------------------------------

/// Visibility check.
///
/// Returns `false` for nodes with zero-or-negative bounds (early reject).
/// Returns `true` when tree.bounds is unknown (`w<=0||h<=0`) — conservative
/// pass: a node with sensible bounds shouldn't be filtered just because we
/// don't have a viewport to clip against. Otherwise checks for any
/// non-empty rectangle intersection between `node.bounds` and `tree.bounds`.
///
/// Pure / branch-only / no allocations — LLVM should inline aggressively.
#[inline]
#[must_use]
pub fn is_visible_enough(node: &A11yNode, tree: &A11yNode) -> bool {
    let b = node.bounds;
    if b.w <= 0.0 || b.h <= 0.0 {
        return false;
    }
    let root = tree.bounds;
    if root.w <= 0.0 || root.h <= 0.0 {
        return true; // unknown root, conservative pass
    }
    let x1 = b.x.max(root.x);
    let y1 = b.y.max(root.y);
    let x2 = (b.x + b.w).min(root.x + root.w);
    let y2 = (b.y + b.h).min(root.y + root.h);
    x2 > x1 && y2 > y1
}

/// Intersection area in logical points².
///
/// Used by resolver multi-candidate sorting (favor truly visible elements
/// over partial-offscreen residuals). Returns `0.0` for zero-bounds
/// nodes; returns `b.w * b.h` when tree.bounds is unknown.
#[inline]
#[must_use]
pub fn visible_area(node: &A11yNode, tree: &A11yNode) -> f64 {
    let b = node.bounds;
    if b.w <= 0.0 || b.h <= 0.0 {
        return 0.0;
    }
    let root = tree.bounds;
    if root.w <= 0.0 || root.h <= 0.0 {
        return b.w * b.h;
    }
    let x1 = b.x.max(root.x);
    let y1 = b.y.max(root.y);
    let x2 = (b.x + b.w).min(root.x + root.w);
    let y2 = (b.y + b.h).min(root.y + root.h);
    if x2 <= x1 || y2 <= y1 {
        return 0.0;
    }
    (x2 - x1) * (y2 - y1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(
        raw_type: &str,
        id: Option<&str>,
        label: Option<&str>,
        children: Vec<A11yNode>,
    ) -> A11yNode {
        A11yNode {
            raw_type: raw_type.into(),
            element_type_raw: 1,
            role: role_from_raw_type(raw_type),
            identifier: id.map(str::to_string),
            label: label.map(str::to_string),
            title: None,
            placeholder_value: None,
            value: None,
            text: None,
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                w: 10.0,
                h: 10.0,
            },
            enabled: true,
            selected: false,
            has_focus: false,
            visible: true,
            children,
        }
    }

    /// A real app tree, in miniature: the button is buried under layers of
    /// anonymous layout. Measured on a stock Settings screen, 120 of 166
    /// visible nodes look like these wrappers.
    fn scaffolded_tree() -> A11yNode {
        node(
            "application",
            Some("com.apple.Preferences"),
            Some("Settings"),
            vec![node(
                "window",
                None,
                None,
                vec![node(
                    "other",
                    None,
                    None,
                    vec![node(
                        "other",
                        None,
                        None,
                        vec![node(
                            "other",
                            None,
                            None,
                            vec![node(
                                "button",
                                Some("com.apple.settings.general"),
                                Some("General"),
                                vec![],
                            )],
                        )],
                    )],
                )],
            )],
        )
    }

    #[test]
    fn anonymous_scaffolding_does_not_take_up_the_reader_s_slots() {
        // The failure surface asks for ten. Spending nine on `other → other →
        // other` tells the reader nothing about the screen.
        let got = collect_visible_summaries(&scaffolded_tree(), 10);
        assert!(
            got.iter().all(|s| s.role.is_some()
                || s.name.is_some()
                || s.id.is_some()
                || s.text.is_some()),
            "every entry must be something a reader can recognize; got {got:?}"
        );
    }

    #[test]
    fn the_element_a_reader_wants_survives_a_small_limit() {
        // Before the filter, a limit of 3 returned application + window +
        // other, and the button — the thing you would tap — never appeared.
        let got = collect_visible_summaries(&scaffolded_tree(), 3);
        assert!(
            got.iter()
                .any(|s| s.id.as_deref() == Some("com.apple.settings.general")),
            "the named button must reach a short list; got {got:?}"
        );
    }

    #[test]
    fn a_node_with_an_id_but_no_role_still_counts() {
        // Role is None for uncurated types. An id alone is plenty to act on.
        let tree = node("other", Some("qa-bubble"), None, vec![]);
        let got = collect_visible_summaries(&tree, 10);
        assert_eq!(got.len(), 1, "an id is identity enough; got {got:?}");
    }

    #[test]
    fn invisible_and_disabled_nodes_stay_out() {
        let mut hidden = node("button", Some("hidden-btn"), None, vec![]);
        hidden.visible = false;
        let mut off = node("button", Some("disabled-btn"), None, vec![]);
        off.enabled = false;
        let tree = node("application", Some("app"), None, vec![hidden, off]);
        let got = collect_visible_summaries(&tree, 10);
        assert_eq!(
            got.len(),
            1,
            "only the app node is visible+enabled; got {got:?}"
        );
    }
}
