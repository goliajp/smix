#![doc = include_str!("../README.md")]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

//! smix-selector-resolver — Selector resolution against an [`A11yNode`]
//! tree (stone, hot path).
//!
//! Ported from now-retired TS source: `src/core/resolve-selector.ts` (279 lines, v1.5
//! c5i-d + v1.6 c4 lock-in). 1:1 semantics:
//!
//! 1. **Collect**: DFS pre-order over the tree, collect every node whose
//!    base form matches (text / id / label / role+name / focused / anchor
//!    accept-all).
//! 2. **Visibility filter** (v1.5 c5i-d): drop nodes whose bounds are
//!    zero or completely outside the tree viewport — unless no candidate
//!    is visible, then drop nothing (TS semantic preserves miss reports).
//! 3. **Spatial filter** (v0.3 C5): for each
//!    `near/below/above/leftOf/rightOf/inside` key (top-level for
//!    base 1-4, or inside `anchor.*` for base 6), recursively resolve
//!    the anchor sub-selector; filter candidates by centroid-axis or
//!    geometric containment. AND semantics. Anchor null → overall null.
//! 4. **Index pick** (v0.3 C5): apply `first/last/nth` in declaration
//!    order; later overrides earlier (TS line 273-278 — `nth` wins
//!    if both `first` and `nth` set).
//!
//! # Pattern compile cache
//!
//! Selector trees carry [`Pattern`] (wire form). Every text/regex match
//! site re-compiles the regex unless cached. The resolver builds a
//! per-call `ResolverContext` that walks the selector tree once, calls
//! [`Pattern::compile`] on every [`Pattern`] node, and stores the
//! resulting [`CompiledPattern`] keyed by the raw pointer to the wire
//! `Pattern` (stable for the lifetime of the borrowed selector).
//!
//! This is the key perf gain over the TS resolver (which lacks an
//! explicit cache — V8 RegExp object internally lazy-compiles, but every
//! `matchText` call still pays the per-candidate fold). For a 100-node
//! tree with a single text base, Rust 1× compile + 100 × match_compiled
//! ≪ TS 100 × compile-and-match.

#![doc(html_root_url = "https://docs.smix.dev/smix-selector-resolver")]

use smix_screen::{A11yNode, Rect, Role, is_visible_enough};
use smix_selector::{
    AnchorBox, CompiledPattern, Modifiers, Pattern, Selector, match_text_compiled,
};
use std::collections::HashMap;

const NEAR_THRESHOLD_PT: f64 = 100.0; // logical points @1x (1:1 跟 TS line 188)

// -------------------- public API -----------------------------------------

/// Resolve a [`Selector`] against an [`A11yNode`] tree. Returns the first
/// surviving candidate after collect → visibility → spatial → index.
///
/// Returns `None` when the selector has no matching node OR any
/// transitive regex compilation fails (TS resolver throws on regex
/// error; we choose silent None — caller can `Pattern::compile()` first
/// to surface compile errors explicitly).
#[must_use]
pub fn resolve_selector<'tree>(
    tree: &'tree A11yNode,
    selector: &Selector,
) -> Option<&'tree A11yNode> {
    let ctx = ResolverContext::new(selector)?;
    resolve_selector_compiled(tree, selector, &ctx)
}

/// Resolve all matching candidates (TS `resolveSelectorAll`, line 89-96).
///
/// Same pipeline as [`resolve_selector`] but skips the final index pick —
/// `first/last/nth` are silently ignored when present (Playwright
/// `locator(...).all()` semantics: "find all" is incompatible with "pick
/// one by position").
#[must_use]
pub fn resolve_selector_all<'tree>(
    tree: &'tree A11yNode,
    selector: &Selector,
) -> Vec<&'tree A11yNode> {
    let Some(ctx) = ResolverContext::new(selector) else {
        return vec![];
    };
    resolve_selector_all_compiled(tree, selector, &ctx)
}

/// Resolve a [`Selector`] against an [`A11yNode`] tree using a caller-
/// provided [`ResolverContext`]. Same pipeline as [`resolve_selector`]
/// but skips the per-call cache build — pass a reusable context built
/// once via [`ResolverContext::new`] to amortize regex compile cost
/// across many calls (typical retry loop in `smix-driver::wait_for` /
/// `scroll`). See [`ResolverContext`] doc for the lifetime contract.
#[must_use]
pub fn resolve_selector_compiled<'tree>(
    tree: &'tree A11yNode,
    selector: &Selector,
    ctx: &ResolverContext,
) -> Option<&'tree A11yNode> {
    resolve_inner(tree, selector, ctx).into_iter().next()
}

/// Resolve all matching candidates with a caller-provided
/// [`ResolverContext`]. Same as [`resolve_selector_all`] minus the per-
/// call cache build. See [`resolve_selector_compiled`] for the reuse
/// pattern.
#[must_use]
pub fn resolve_selector_all_compiled<'tree>(
    tree: &'tree A11yNode,
    selector: &Selector,
    ctx: &ResolverContext,
) -> Vec<&'tree A11yNode> {
    resolve_inner_no_index(tree, selector, ctx)
}

// -------------------- ResolverContext (compile cache) --------------------

/// Per-call cache mapping raw pointer of each [`Pattern`] in the selector
/// tree to its [`CompiledPattern`]. Built once at entry; lookup is O(1)
/// hash.
///
/// # Cache reuse across calls (v3.31 c1)
///
/// Build a single `ResolverContext` outside a retry loop and pass it to
/// [`resolve_selector_compiled`] / [`resolve_selector_all_compiled`] on
/// every iteration to skip the per-call regex compile prepass. The
/// regex hit case drops from ~9.5 µs/iter to ~260 ns/iter on a 15-node
/// tree (see `PERFORMANCE.md` v3.31 c1 segment). The convenience
/// wrappers [`resolve_selector`] / [`resolve_selector_all`] keep the
/// per-call construction for one-shot callers.
///
/// # Safety
///
/// `*const Pattern` is used only as a HashMap key — never dereferenced.
/// The borrowed [`Selector`] passed to [`ResolverContext::new`] must
/// outlive every subsequent `resolve_selector_compiled` call that uses
/// this context. Pattern addresses stay stable for the lifetime of the
/// outer `&Selector` borrow. Passing a context built from a different
/// `Selector` than the one being resolved is a runtime contract
/// violation (cache misses degrade to silent `None` from `ctx.pattern`).
pub struct ResolverContext {
    compiled: HashMap<*const Pattern, CompiledPattern>,
}

// SAFETY: `*const Pattern` is used only as a HashMap key — never
// dereferenced. The borrowed Selector (whose Pattern addresses are
// captured at `new`) outlives the context per the documented lifetime
// contract. Pointer values are inert bits across threads, so moving a
// ResolverContext between threads (e.g. a tokio worker carrying the
// future built by `smix-driver::wait_for` / `scroll`) is safe.
// CompiledPattern itself is already Send + Sync (regex::Regex +
// String).
unsafe impl Send for ResolverContext {}
unsafe impl Sync for ResolverContext {}

impl ResolverContext {
    /// Build a context by walking the selector tree once and pre-compiling
    /// every embedded [`Pattern`]. Returns `None` if any regex pattern
    /// fails to compile (matches the silent-`None` semantic of
    /// [`resolve_selector`]).
    pub fn new(selector: &Selector) -> Option<Self> {
        let mut compiled = HashMap::new();
        if !Self::compile_selector(selector, &mut compiled) {
            return None;
        }
        Some(ResolverContext { compiled })
    }

    /// Walks the selector tree once, compiles every [`Pattern`]. Returns
    /// false on any compile error.
    fn compile_selector(
        selector: &Selector,
        out: &mut HashMap<*const Pattern, CompiledPattern>,
    ) -> bool {
        match selector {
            Selector::Text { text, modifiers } => {
                if !Self::cache_pattern(text, out) {
                    return false;
                }
                Self::compile_modifiers(modifiers, out)
            }
            Selector::Id { modifiers, .. } | Selector::Label { modifiers, .. } => {
                Self::compile_modifiers(modifiers, out)
            }
            Selector::Role {
                name, modifiers, ..
            } => {
                if let Some(name_pat) = name
                    && !Self::cache_pattern(name_pat, out)
                {
                    return false;
                }
                Self::compile_modifiers(modifiers, out)
            }
            Selector::Focused { .. } => true,
            Selector::Anchor { anchor, .. } => Self::compile_anchor(anchor, out),
            Selector::LocalizedText { modifiers, .. } => {
                // v5.18 c1 — adapter is expected to desugar LocalizedText →
                // Selector::Text before invoking the resolver. The variant
                // reaches compile only when the caller forgot to desugar
                // (test path / direct SDK use without adapter). Compile the
                // modifiers so the call doesn't crash; the actual match
                // (matches_base) will return false for any node, and the
                // caller will see "no match" — debuggable via describe_selector.
                Self::compile_modifiers(modifiers, out)
            }
            Selector::OcrText { modifiers, .. } => {
                // v5.19 c1 — adapter handles OcrText dispatch directly via
                // App::find_by_text_ocr + tap_at_norm_coord, bypassing the
                // resolver pipeline. Variant reaches resolver only when
                // adapter forgot to dispatch; treat same as LocalizedText
                // (compile modifiers; matches_base returns false).
                Self::compile_modifiers(modifiers, out)
            }
            Selector::AnchorRelative { anchor, .. } => {
                // v5.20 c1 — adapter dispatches AnchorRelative directly via
                // App::find_norm_coord(anchor) + tap_at_norm_coord, never
                // calls resolver on the AnchorRelative itself. But the
                // ANCHOR sub-selector reaches the resolver through SDK
                // App::find — must compile its patterns recursively.
                Self::compile_selector(anchor, out)
            }
            Selector::Point { .. } => true,
            Selector::Fallback { fallback } => {
                // v5.20 c2 — compile every chain element's patterns; adapter
                // iterates chain in dispatch but each sub-selector may need
                // its own pattern cache. Returns false on first compile
                // failure (matches LocalizedText / OcrText semantics).
                fallback.iter().all(|s| Self::compile_selector(s, out))
            }
        }
    }

    fn cache_pattern(p: &Pattern, out: &mut HashMap<*const Pattern, CompiledPattern>) -> bool {
        let key = p as *const Pattern;
        if out.contains_key(&key) {
            return true;
        }
        match p.compile() {
            Ok(cp) => {
                out.insert(key, cp);
                true
            }
            Err(_) => false,
        }
    }

    fn compile_modifiers(
        m: &Modifiers,
        out: &mut HashMap<*const Pattern, CompiledPattern>,
    ) -> bool {
        let slots = [
            m.near.as_deref(),
            m.below.as_deref(),
            m.above.as_deref(),
            m.left_of.as_deref(),
            m.right_of.as_deref(),
            m.inside.as_deref(),
            m.ancestor.as_deref(),
        ];
        for child in slots.iter().flatten() {
            if !Self::compile_selector(child, out) {
                return false;
            }
        }
        true
    }

    fn compile_anchor(a: &AnchorBox, out: &mut HashMap<*const Pattern, CompiledPattern>) -> bool {
        let slots = [
            a.near.as_deref(),
            a.below.as_deref(),
            a.above.as_deref(),
            a.left_of.as_deref(),
            a.right_of.as_deref(),
            a.inside.as_deref(),
        ];
        for child in slots.iter().flatten() {
            if !Self::compile_selector(child, out) {
                return false;
            }
        }
        true
    }

    /// O(1) cache lookup. Returns `None` if `p` was not seen during the
    /// build prepass — typically means `p` belongs to a different
    /// selector tree than the one passed to [`ResolverContext::new`].
    pub fn pattern(&self, p: &Pattern) -> Option<&CompiledPattern> {
        self.compiled.get(&(p as *const Pattern))
    }
}

// -------------------- resolve pipeline -----------------------------------

fn resolve_inner<'tree>(
    tree: &'tree A11yNode,
    selector: &Selector,
    ctx: &ResolverContext,
) -> Vec<&'tree A11yNode> {
    let raw = dfs_collect(tree, |n| matches_base(n, selector, ctx));
    let visible: Vec<&A11yNode> = raw
        .into_iter()
        .filter(|n| is_visible_enough(n, tree))
        .collect();
    let topmost = topmost_modal_filter(tree, visible);
    // v3.14 c1.6 / v3.19 c2 — explicit structural intent (ancestor /
    // spatial modifier) overrides implicit interactive preference
    // (tappable filter). If a non-tappable candidate is the one that
    // satisfies the user-provided structural / spatial constraint and a
    // tappable sibling sits in a position that fails it, tappable-first
    // would drop the non-tappable, leaving only the failing tappable →
    // empty result. Pipeline: ancestor → spatial → tappable → index.
    let Some(after_ancestor) = apply_ancestor_filter(tree, topmost, selector) else {
        return vec![];
    };
    let Some(after_spatial) = apply_spatial_filters(tree, after_ancestor, selector, ctx) else {
        return vec![];
    };
    let tappable = tappable_subset_filter(after_spatial);
    apply_index(tappable, selector)
}

fn resolve_inner_no_index<'tree>(
    tree: &'tree A11yNode,
    selector: &Selector,
    ctx: &ResolverContext,
) -> Vec<&'tree A11yNode> {
    let raw = dfs_collect(tree, |n| matches_base(n, selector, ctx));
    let visible: Vec<&A11yNode> = raw
        .into_iter()
        .filter(|n| is_visible_enough(n, tree))
        .collect();
    let topmost = topmost_modal_filter(tree, visible);
    let Some(after_ancestor) = apply_ancestor_filter(tree, topmost, selector) else {
        return vec![];
    };
    let Some(after_spatial) = apply_spatial_filters(tree, after_ancestor, selector, ctx) else {
        return vec![];
    };
    tappable_subset_filter(after_spatial)
}

// v3.5 c1 — tappable preference. When candidate set mixes tappable
// (button / link / cell / tab / menuItem) and non-tappable (alert /
// staticText / window / group / ...), drop the non-tappable so a single
// label selector picks the actual interactive element, not the alert
// container or its title. Same semantic as `XCUIElementQuery.firstMatch`
// which favours hit-testable leaves over their surrounding chrome.
// Single-kind candidate lists fall through untouched (v1.x behaviour).
fn tappable_subset_filter(candidates: Vec<&A11yNode>) -> Vec<&A11yNode> {
    if candidates.len() <= 1 {
        return candidates;
    }
    let is_tappable = |n: &A11yNode| -> bool {
        matches!(
            n.raw_type.as_str(),
            "button" | "link" | "cell" | "tab" | "menuItem"
        )
    };
    let has_tappable = candidates.iter().any(|n| is_tappable(n));
    let has_non_tappable = candidates.iter().any(|n| !is_tappable(n));
    if has_tappable && has_non_tappable {
        candidates.into_iter().filter(|n| is_tappable(n)).collect()
    } else {
        candidates
    }
}

// v3.5 c1 — topmost hit-test (Apple XCUIElementQuery.firstMatch +
// maestro findElement semantic): when ≥1 candidate is inside a
// Role::Alert / Role::Dialog subtree (modal overlay in play), drop
// the candidates that live under the underlying drawer/page so a
// single selector picks the modal button. When no modal overlay is
// present, the input list passes through unchanged — v1.x DFS
// pre-order behaviour stays intact.
// Modal container detection. The Swift `/tree` route does not emit a `role`
// field (only `rawType`, see TreeRoute.swift `nodeToDict`) — so Rust-side
// `Role::Alert` / `Role::Dialog` are always None on real-sim payloads.
// Match `raw_type` strings directly (lower-case wire shape, mirrors
// `elementTypeName(7 / 8)`); accept the `Role` enum too so unit-test
// fixtures that set `role` only stay valid.
fn is_modal_node(n: &A11yNode) -> bool {
    matches!(n.raw_type.as_str(), "alert" | "dialog")
        || matches!(n.role, Some(Role::Alert) | Some(Role::Dialog))
}

fn tree_has_modal_role(node: &A11yNode) -> bool {
    if is_modal_node(node) {
        return true;
    }
    node.children.iter().any(tree_has_modal_role)
}

fn topmost_modal_filter<'tree>(
    tree: &'tree A11yNode,
    candidates: Vec<&'tree A11yNode>,
) -> Vec<&'tree A11yNode> {
    // Fast path: ≤1 candidate has nothing to disambiguate; trees with no
    // Alert/Dialog node anywhere fall back to v1.x DFS pre-order with
    // zero overhead (a single shallow tree walk via `any`).
    if candidates.len() <= 1 || !tree_has_modal_role(tree) {
        return candidates;
    }
    let mut parent: HashMap<*const A11yNode, &'tree A11yNode> = HashMap::new();
    fn walk<'tree>(n: &'tree A11yNode, parent: &mut HashMap<*const A11yNode, &'tree A11yNode>) {
        for c in &n.children {
            parent.insert(c as *const A11yNode, n);
            walk(c, parent);
        }
    }
    walk(tree, &mut parent);

    let in_modal_subtree = |start: &A11yNode| -> bool {
        if is_modal_node(start) {
            return true;
        }
        let mut cur: *const A11yNode = start;
        while let Some(p) = parent.get(&cur) {
            if is_modal_node(p) {
                return true;
            }
            cur = *p as *const A11yNode;
        }
        false
    };

    let in_modal: Vec<&'tree A11yNode> = candidates
        .iter()
        .copied()
        .filter(|n| in_modal_subtree(n))
        .collect();
    if in_modal.is_empty() {
        candidates
    } else {
        in_modal
    }
}

fn dfs_collect<'tree, F>(tree: &'tree A11yNode, pred: F) -> Vec<&'tree A11yNode>
where
    F: Fn(&A11yNode) -> bool,
{
    let mut out: Vec<&'tree A11yNode> = Vec::new();
    fn walk<'tree, F: Fn(&A11yNode) -> bool>(
        n: &'tree A11yNode,
        pred: &F,
        out: &mut Vec<&'tree A11yNode>,
    ) {
        if pred(n) {
            out.push(n);
        }
        for c in &n.children {
            walk(c, pred, out);
        }
    }
    walk(tree, &pred, &mut out);
    out
}

fn matches_base(node: &A11yNode, selector: &Selector, ctx: &ResolverContext) -> bool {
    match selector {
        // v1.5 C2 — anchor-only base: every node is a candidate.
        Selector::Anchor { .. } => true,
        Selector::Text { text, .. } => match ctx.pattern(text) {
            Some(cp) => match_text_compiled(node, cp),
            None => false,
        },
        Selector::Id { id, .. } => {
            if id.is_empty() {
                return false;
            }
            node.identifier.as_deref() == Some(id.as_str())
        }
        Selector::Label { label, .. } => {
            if label.is_empty() {
                return false;
            }
            node.label.as_deref() == Some(label.as_str())
        }
        Selector::Role { role, name, .. } => {
            if node.role != Some(*role) {
                return false;
            }
            match name {
                None => true,
                Some(name_pat) => match ctx.pattern(name_pat) {
                    Some(cp) => match_text_compiled(node, cp),
                    None => false,
                },
            }
        }
        Selector::Focused { .. } => node.has_focus,
        // v5.18 c1 — adapter is expected to desugar LocalizedText → Text
        // before resolving. If we somehow get here (unit-test path / direct
        // SDK use), no node matches — describe_selector still renders this
        // variant for AI-readable error output.
        Selector::LocalizedText { .. } => false,
        // v5.19 c1 — adapter dispatches OcrText directly via OCR + tap_at_coord
        // and never invokes the resolver pipeline. Variant reaches here only
        // when adapter forgot to dispatch; no node matches.
        Selector::OcrText { .. } => false,
        // v5.20 c1 — adapter dispatches AnchorRelative directly (resolve anchor
        // sub-selector → norm coord + dx/dy → tap_at_norm_coord). Variant
        // never reaches here through the standard pipeline; if it somehow
        // does, no node matches.
        Selector::AnchorRelative { .. } => false,
        // v5.20 c2 — Point + Fallback are dispatched by adapter without
        // resolver involvement. Reaching matches_base means a caller
        // forgot to dispatch; no node matches.
        Selector::Point { .. } | Selector::Fallback { .. } => false,
    }
}

// -------------------- v3.14 c1 — ancestor modifier (G7) ------------------

// Ancestor-chain filter. Keep only candidates whose a11y tree parent chain
// (recursive ancestors, excluding self) contains at least one node matching
// the selector's `Modifiers::ancestor` sub-selector. Ancestor sub-selector
// resolving to empty short-circuits the whole resolve to None (跟 spatial
// anchor null 同语义).
//
// Different from `inside` spatial modifier (geometric bounds-containment):
// `ancestor` walks the a11y tree parent chain — structural filter, not spatial.
// Same parent-map idiom as `topmost_modal_filter` (line 283 段).
//
// `Selector::Anchor` / `Selector::Focused` have no `Modifiers::ancestor`
// field on their wire shape (anchor base is already a spatial intent,
// focused is runtime-resolved without modifiers), so they passthrough.
fn apply_ancestor_filter<'tree>(
    tree: &'tree A11yNode,
    candidates: Vec<&'tree A11yNode>,
    selector: &Selector,
) -> Option<Vec<&'tree A11yNode>> {
    let ancestor_sel = match selector {
        Selector::Text { modifiers, .. }
        | Selector::Id { modifiers, .. }
        | Selector::Label { modifiers, .. }
        | Selector::Role { modifiers, .. }
        | Selector::LocalizedText { modifiers, .. }
        | Selector::OcrText { modifiers, .. } => modifiers.ancestor.as_deref(),
        // v5.20 c1 — AnchorRelative has no Modifiers (anchor sub-selector
        // carries its own); adapter dispatches directly, ancestor n/a.
        // v5.20 c2 — Point + Fallback no Modifiers either.
        Selector::Anchor { .. }
        | Selector::Focused { .. }
        | Selector::AnchorRelative { .. }
        | Selector::Point { .. }
        | Selector::Fallback { .. } => None,
    };
    let Some(ancestor_sel) = ancestor_sel else {
        return Some(candidates);
    };
    let anchor = resolve_selector(tree, ancestor_sel)?;

    let mut parent: HashMap<*const A11yNode, &'tree A11yNode> = HashMap::new();
    fn walk<'tree>(n: &'tree A11yNode, parent: &mut HashMap<*const A11yNode, &'tree A11yNode>) {
        for c in &n.children {
            parent.insert(c as *const A11yNode, n);
            walk(c, parent);
        }
    }
    walk(tree, &mut parent);

    let surviving: Vec<&'tree A11yNode> = candidates
        .into_iter()
        .filter(|c| {
            if std::ptr::eq(*c, anchor) {
                return false;
            }
            let mut cur: *const A11yNode = *c;
            while let Some(p) = parent.get(&cur) {
                if std::ptr::eq(*p, anchor) {
                    return true;
                }
                cur = *p as *const A11yNode;
            }
            false
        })
        .collect();
    Some(surviving)
}

// -------------------- spatial filter -------------------------------------

#[derive(Clone, Copy)]
enum SpatialKey {
    Near,
    Below,
    Above,
    LeftOf,
    RightOf,
    Inside,
}

const SPATIAL_KEYS: [SpatialKey; 6] = [
    SpatialKey::Near,
    SpatialKey::Below,
    SpatialKey::Above,
    SpatialKey::LeftOf,
    SpatialKey::RightOf,
    SpatialKey::Inside,
];

fn get_spatial(selector: &Selector, key: SpatialKey) -> Option<&Selector> {
    match selector {
        Selector::Anchor { anchor, .. } => match key {
            SpatialKey::Near => anchor.near.as_deref(),
            SpatialKey::Below => anchor.below.as_deref(),
            SpatialKey::Above => anchor.above.as_deref(),
            SpatialKey::LeftOf => anchor.left_of.as_deref(),
            SpatialKey::RightOf => anchor.right_of.as_deref(),
            SpatialKey::Inside => anchor.inside.as_deref(),
        },
        Selector::Text { modifiers, .. }
        | Selector::Id { modifiers, .. }
        | Selector::Label { modifiers, .. }
        | Selector::Role { modifiers, .. }
        | Selector::LocalizedText { modifiers, .. }
        | Selector::OcrText { modifiers, .. } => match key {
            SpatialKey::Near => modifiers.near.as_deref(),
            SpatialKey::Below => modifiers.below.as_deref(),
            SpatialKey::Above => modifiers.above.as_deref(),
            SpatialKey::LeftOf => modifiers.left_of.as_deref(),
            SpatialKey::RightOf => modifiers.right_of.as_deref(),
            SpatialKey::Inside => modifiers.inside.as_deref(),
        },
        Selector::Focused { .. }
        | Selector::AnchorRelative { .. }
        | Selector::Point { .. }
        | Selector::Fallback { .. } => None,
    }
}

fn apply_spatial_filters<'tree>(
    tree: &'tree A11yNode,
    candidates: Vec<&'tree A11yNode>,
    selector: &Selector,
    _ctx: &ResolverContext,
) -> Option<Vec<&'tree A11yNode>> {
    let mut surviving = candidates;
    for key in &SPATIAL_KEYS {
        let Some(anchor_sel) = get_spatial(selector, *key) else {
            continue;
        };
        // Recursive anchor resolution. Each anchor sub-selector compiles
        // its own ResolverContext (Pattern lifetimes are local — caller's
        // Pattern pointers don't overlap recursively because Box<Selector>
        // makes each sub-tree a fresh node tree).
        let Some(anchor) = resolve_selector(tree, anchor_sel) else {
            return None; // anchor null short-circuits whole resolve to None
        };
        surviving.retain(|c| satisfies(*key, c, anchor));
    }
    Some(surviving)
}

fn satisfies(key: SpatialKey, c: &A11yNode, a: &A11yNode) -> bool {
    if std::ptr::eq(c, a) {
        return false;
    }
    let cc = centroid(c.bounds);
    let ac = centroid(a.bounds);
    match key {
        SpatialKey::Near => dist(cc, ac) <= NEAR_THRESHOLD_PT,
        SpatialKey::Below => cc.1 > ac.1,
        SpatialKey::Above => cc.1 < ac.1,
        SpatialKey::LeftOf => cc.0 < ac.0,
        SpatialKey::RightOf => cc.0 > ac.0,
        SpatialKey::Inside => contains(a.bounds, c.bounds),
    }
}

#[inline]
fn centroid(r: Rect) -> (f64, f64) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

#[inline]
fn dist(p: (f64, f64), q: (f64, f64)) -> f64 {
    let dx = p.0 - q.0;
    let dy = p.1 - q.1;
    (dx * dx + dy * dy).sqrt()
}

#[inline]
fn contains(outer: Rect, inner: Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x + inner.w <= outer.x + outer.w
        && inner.y + inner.h <= outer.y + outer.h
}

// -------------------- index pick -----------------------------------------

fn apply_index<'tree>(list: Vec<&'tree A11yNode>, selector: &Selector) -> Vec<&'tree A11yNode> {
    let (first, last, nth) = match selector {
        Selector::Anchor { index, .. } => (index.first, index.last, index.nth),
        Selector::Text { modifiers, .. }
        | Selector::Id { modifiers, .. }
        | Selector::Label { modifiers, .. }
        | Selector::Role { modifiers, .. }
        | Selector::LocalizedText { modifiers, .. }
        | Selector::OcrText { modifiers, .. } => (modifiers.first, modifiers.last, modifiers.nth),
        Selector::Focused { .. }
        | Selector::AnchorRelative { .. }
        | Selector::Point { .. }
        | Selector::Fallback { .. } => return list,
    };
    let has_first = first == Some(true);
    let has_last = last == Some(true);
    let has_nth = nth.is_some();
    if !has_first && !has_last && !has_nth {
        return list;
    }
    // TS semantics (line 274-278): first → list[0], last overrides → list[len-1],
    // nth overrides both → list[nth]. Single-shot return [picked] or [].
    let picked: Option<&'tree A11yNode> = if has_nth {
        nth.and_then(|i| list.get(i).copied())
    } else if has_last {
        list.last().copied()
    } else {
        // has_first
        list.first().copied()
    };
    match picked {
        Some(n) => vec![n],
        None => vec![],
    }
}
