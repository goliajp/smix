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
/// list of visible+enabled [`ElementSummary`] entries; `screenshot` is
/// an optional base64 PNG; `frontApp` / `summary` / `captured_at` are
/// caller-populated metadata.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenDescription {
    /// Optional base64-encoded PNG screenshot of the screen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<String>,
    /// Visible+enabled elements in DFS pre-order.
    pub elements: Vec<ElementSummary>,
    /// Bundle id of the frontmost app at capture time.
    pub front_app: String,
    /// Free-form one-line summary (caller-populated).
    pub summary: String,
    /// Wall-clock capture timestamp (Unix epoch milliseconds).
    pub captured_at: f64,
}

/// Collect up to `limit` visible+enabled nodes (DFS pre-order), projecting
/// each via [`summarize_node`]. Default limit = 1000.
#[must_use]
pub fn collect_visible_summaries(tree: &A11yNode, limit: usize) -> Vec<ElementSummary> {
    let mut out: Vec<ElementSummary> = Vec::new();
    fn walk(n: &A11yNode, limit: usize, out: &mut Vec<ElementSummary>) {
        if out.len() >= limit {
            return;
        }
        if n.enabled && n.visible {
            out.push(summarize_node(n));
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

/// Accessibility tree node.
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

/// Older wire payloads predate the `elementTypeRaw` field; default to
/// 1 (`.other`), which is the safest fallback and matches how Swift
/// `elementTypeName` treats unknown raw values.
fn default_element_type_raw() -> u64 { 1 }

/// Single-node accessibility snapshot returned by `/tree`. Fields
/// mirror the Swift-side `TreeRoute.nodeToDict` shape.
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
