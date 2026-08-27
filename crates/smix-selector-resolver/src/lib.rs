#![doc = include_str!("../README.md")]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

//! smix-selector-resolver — Selector resolution against an [`A11yNode`]
//! tree (stone, hot path).
//!
//! Resolution pipeline:
//!
//! 1. **Collect**: DFS pre-order over the tree, collect every node whose
//!    base form matches (text / id / label / role+name / focused / anchor
//!    accept-all).
//! 2. **Visibility filter**: drop nodes whose bounds are
//!    zero or completely outside the tree viewport — unless no candidate
//!    is visible, then drop nothing, which preserves miss reports.
//! 3. **Spatial filter**: for each
//!    `near/below/above/leftOf/rightOf/inside` key (top-level for
//!    base 1-4, or inside `anchor.*` for base 6), recursively resolve
//!    the anchor sub-selector; filter candidates by centroid-axis or
//!    geometric containment. AND semantics. Anchor null → overall null.
//! 4. **Index pick**: apply `first/last/nth` in declaration
//!    order; later overrides earlier — `nth` wins if both `first` and
//!    `nth` are set.
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
//! This cache is the pipeline's key perf gain: for a 100-node tree with
//! a single text base, 1× compile + 100 × match_compiled costs far less
//! than 100 × compile-and-match.

#![doc(html_root_url = "https://docs.smix.dev/smix-selector-resolver")]

use smix_screen::{A11yNode, Rect, Role, is_visible_enough};
use smix_selector::{
    AnchorBox, CompiledPattern, Modifiers, Pattern, Selector, match_text_compiled,
};
use std::collections::HashMap;

const NEAR_THRESHOLD_PT: f64 = 100.0; // logical points @1x

// -------------------- public API -----------------------------------------

/// Resolve a [`Selector`] against an [`A11yNode`] tree. Returns the first
/// surviving candidate after collect → visibility → spatial → index.
///
/// Returns `None` when the selector has no matching node OR any
/// transitive regex compilation fails — a compile error yields a silent
/// `None` rather than an error, so callers who need to surface compile
/// failures explicitly should `Pattern::compile()` first.
#[must_use]
pub fn resolve_selector<'tree>(
    tree: &'tree A11yNode,
    selector: &Selector,
) -> Option<&'tree A11yNode> {
    let ctx = ResolverContext::new(selector)?;
    resolve_selector_compiled(tree, selector, &ctx)
}

/// Resolve all matching candidates.
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
/// # Cache reuse across calls
///
/// Build a single `ResolverContext` outside a retry loop and pass it to
/// [`resolve_selector_compiled`] / [`resolve_selector_all_compiled`] on
/// every iteration to skip the per-call regex compile prepass. The
/// regex hit case drops from ~9.5 µs/iter to ~260 ns/iter on a 15-node
/// tree (see `BUDGETS.md`). The convenience
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
                // The adapter is expected to desugar LocalizedText →
                // Selector::Text before invoking the resolver. The variant
                // reaches compile only when the caller forgot to desugar
                // (test path / direct SDK use without adapter). Compile the
                // modifiers so the call doesn't crash; the actual match
                // (matches_base) will return false for any node, and the
                // caller will see "no match" — debuggable via describe_selector.
                Self::compile_modifiers(modifiers, out)
            }
            Selector::OcrText { modifiers, .. } => {
                // The adapter handles OcrText dispatch directly via
                // App::find_by_text_ocr + tap_at_norm_coord, bypassing the
                // resolver pipeline. Variant reaches resolver only when
                // adapter forgot to dispatch; treat same as LocalizedText
                // (compile modifiers; matches_base returns false).
                Self::compile_modifiers(modifiers, out)
            }
            Selector::AnchorRelative { anchor, .. } => {
                // The adapter dispatches AnchorRelative directly via
                // App::find_norm_coord(anchor) + tap_at_norm_coord, never
                // calls resolver on the AnchorRelative itself. But the
                // ANCHOR sub-selector reaches the resolver through SDK
                // App::find — must compile its patterns recursively.
                Self::compile_selector(anchor, out)
            }
            Selector::Point { .. } => true,
            Selector::Fallback { fallback } => {
                // Compile what each layer needs, and do not let one
                // layer's unusable pattern take the chain down with it.
                // `all` short-circuited on the first failure, so a
                // chain whose first layer held a malformed regex
                // resolved to nothing — including the layers written
                // precisely so that something would still match.
                // A layer that did not compile simply never matches.
                for layer in fallback {
                    Self::compile_selector(layer, out);
                }
                true
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
        // The conjunction's sub-selectors are matched against the same
        // node, so their patterns need compiling like any other. Left
        // out of the slots above only because those are `Option<Box<_>>`
        // and this is a list; a sub-selector whose pattern never reached
        // the cache resolves to nothing, which reads as "no element is
        // both" rather than as a cache miss.
        for sub in &m.and {
            if !Self::compile_selector(sub, out) {
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
    if let Selector::Fallback { fallback } = selector {
        // Layer by layer, in order, and the first one that matches is
        // the answer — never the union. A chain means "use the first of
        // these that works", so `[id, text]` with both matching is one
        // element, and which one is a promise the caller made.
        //
        // Here rather than at the public entry points because
        // `wait_for` polls through the compiled variant, which is
        // exactly the path `assertVisible` takes. Putting it in one of
        // the two would have left the other answering as before — the
        // shape of the defect this fixes.
        return fallback
            .iter()
            .map(|layer| resolve_inner(tree, layer, ctx))
            .find(|found| !found.is_empty())
            .unwrap_or_default();
    }
    let raw = dfs_collect(tree, |n| matches_base(n, selector, ctx));
    let visible: Vec<&A11yNode> = raw
        .into_iter()
        .filter(|n| is_visible_enough(n, tree))
        .collect();
    let topmost = topmost_modal_filter(tree, visible);
    // Explicit structural intent (ancestor / spatial modifier)
    // overrides implicit interactive preference
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
    if let Selector::Fallback { fallback } = selector {
        return fallback
            .iter()
            .map(|layer| resolve_inner_no_index(tree, layer, ctx))
            .find(|found| !found.is_empty())
            .unwrap_or_default();
    }
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

// Tappable preference. When the candidate set mixes tappable
// (button / link / cell / tab / menuItem) and non-tappable (alert /
// staticText / window / group / ...), drop the non-tappable so a single
// label selector picks the actual interactive element, not the alert
// container or its title. Same semantic as `XCUIElementQuery.firstMatch`
// which favours hit-testable leaves over their surrounding chrome.
// Single-kind candidate lists fall through untouched.
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

// Topmost hit-test (Apple XCUIElementQuery.firstMatch + maestro
// findElement semantic): when ≥1 candidate is inside a
// Role::Alert / Role::Dialog subtree (modal overlay in play), drop
// the candidates that live under the underlying drawer/page so a
// single selector picks the modal button. When no modal overlay is
// present, the input list passes through unchanged — plain DFS
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
    if !matches_conjunction(node, selector, ctx) {
        return false;
    }
    match_base_form(node, selector, ctx)
}

/// The `and` constraints: sub-selectors the candidate itself must also
/// satisfy.
///
/// Checked against the same node rather than a neighbour, which is what
/// separates this from the spatial modifiers and from `ancestor`. Empty
/// for every selector naming one thing, so the common path pays a slice
/// check.
///
/// Without this the constraint is parsed, carried, and ignored — the
/// same wrong answer as dropping it, with a longer paper trail.
fn matches_conjunction(node: &A11yNode, selector: &Selector, ctx: &ResolverContext) -> bool {
    let Some(m) = selector.modifiers() else {
        return true;
    };
    m.and.iter().all(|sub| match_base_form(node, sub, ctx))
}

fn match_base_form(node: &A11yNode, selector: &Selector, ctx: &ResolverContext) -> bool {
    match selector {
        // Anchor-only base: every node is a candidate.
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
        // The adapter is expected to desugar LocalizedText → Text
        // before resolving. If we somehow get here (unit-test path / direct
        // SDK use), no node matches — describe_selector still renders this
        // variant for AI-readable error output.
        Selector::LocalizedText { .. } => false,
        // The adapter dispatches OcrText directly via OCR + tap_at_coord
        // and never invokes the resolver pipeline. The variant reaches here
        // only when the adapter forgot to dispatch; no node matches.
        Selector::OcrText { .. } => false,
        // The adapter dispatches AnchorRelative directly (resolve anchor
        // sub-selector → norm coord + dx/dy → tap_at_norm_coord). Variant
        // never reaches here through the standard pipeline; if it somehow
        // does, no node matches.
        Selector::AnchorRelative { .. } => false,
        // A point is a coordinate, not a description of a node, so
        // nothing here can match it; the callers that accept one act on
        // the coordinate directly.
        //
        // A chain is resolved at the entry points above, layer by
        // layer, so it never reaches here. It used to be listed
        // alongside the point with a comment saying the adapter
        // dispatched it — an invariant three verbs kept and every other
        // one had never heard of, which is how `assertVisible` with a
        // chain came to match nothing on either platform.
        Selector::Point { .. } | Selector::Fallback { .. } => false,
    }
}

// -------------------- ancestor modifier ----------------------------------

// Ancestor-chain filter. Keep only candidates whose a11y tree parent chain
// (recursive ancestors, excluding self) contains at least one node matching
// the selector's `Modifiers::ancestor` sub-selector. Ancestor sub-selector
// resolving to empty short-circuits the whole resolve to None — same
// semantics as a null spatial anchor.
//
// Different from `inside` spatial modifier (geometric bounds-containment):
// `ancestor` walks the a11y tree parent chain — structural filter, not spatial.
// Same parent-map idiom as `topmost_modal_filter`.
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
        // The anchor form carries its own ancestor sub-selector.
        Selector::Anchor { anchor, .. } => anchor.ancestor.as_deref(),
        // AnchorRelative has no Modifiers (the anchor sub-selector carries
        // its own); the adapter dispatches directly, so ancestor is n/a.
        // Point + Fallback carry no Modifiers either.
        Selector::Focused { .. }
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
    // Precedence: first → list[0], last overrides → list[len-1],
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

/// Turn the probe's semantics payload into the tree everything downstream
/// already speaks.
///
/// Not a second kind of node, and not a second resolver. The semantics tree
/// carries the same facts — an identifier, some text, a rectangle, whether
/// it is enabled — from a source that can see things the accessibility
/// projection cannot. Converting keeps one resolution pipeline, which is
/// what stops "the probe path" and "the a11y path" from drifting into two
/// products.
///
/// The probe reports several roots (a dialog composes into its own), so the
/// result is a synthetic parent over them. Its bounds are the union, which
/// is what a spatial modifier would expect of a screen.
///
/// Returns `None` when the payload is not the shape the probe emits, rather
/// than an empty tree: "nothing on screen" and "I could not read this" want
/// different answers, and one value for both is how a caller learns to
/// distrust the field.
pub fn probe_tree_to_a11y(json: &str) -> Option<A11yNode> {
    let roots: Vec<ProbeNodeWire> = serde_json::from_str(json).ok()?;
    let mut converted: Vec<A11yNode> = roots.iter().map(ProbeNodeWire::to_a11y).collect();
    if let Some(modal) = modal_index(&converted) {
        // A modal is showing, so what is behind it is not addressable —
        // even though the probe can see it perfectly well.
        //
        // Android already hides a modal's background from accessibility and
        // that is not a defect to route around: a user cannot touch those
        // controls either. Reaching them would make smix able to do what
        // the person it stands in for cannot, which is the line C2 drew
        // when it refused a semantics OnClick through a scrim. The probe
        // widens what smix can SEE, never what it can REACH.
        converted = vec![converted.swap_remove(modal)];
    }
    let mut parent = blank_node();
    parent.raw_type = "SemanticsRoots".to_string();
    parent.bounds = union_bounds(&converted);
    parent.children = converted;
    Some(parent)
}

#[derive(serde::Deserialize)]
struct ProbeNodeWire {
    #[serde(default)]
    #[allow(dead_code)]
    id: i64,
    #[serde(default, rename = "testTag")]
    test_tag: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default, rename = "editableText")]
    editable_text: Option<String>,
    /// What a field actually holds. Separate from `editableText`, which on
    /// a masked field reads back as bullets — Compose applies the visual
    /// transformation before semantics sees it.
    #[serde(default, rename = "inputText")]
    input_text: Option<String>,
    #[serde(default, rename = "contentDescription")]
    content_description: Option<String>,
    #[serde(default)]
    bounds: [f64; 4],
    #[serde(default)]
    focused: bool,
    #[serde(default = "yes")]
    enabled: bool,
    #[serde(default)]
    children: Vec<ProbeNodeWire>,
}

fn yes() -> bool {
    true
}

impl ProbeNodeWire {
    fn to_a11y(&self) -> A11yNode {
        let mut n = blank_node();
        n.identifier = self.test_tag.clone();
        n.label = self.content_description.clone();
        n.text = self.text.clone();
        // `inputText` first: it is what was typed, where `editableText` is
        // what is shown. A predicate comparing a masked field with what a
        // flow typed asks a question only the first can answer.
        n.value = self.input_text.clone().or_else(|| self.editable_text.clone());
        n.enabled = self.enabled;
        n.has_focus = self.focused;
        n.bounds = smix_screen::Rect {
            x: self.bounds[0],
            y: self.bounds[1],
            w: (self.bounds[2] - self.bounds[0]).max(0.0),
            h: (self.bounds[3] - self.bounds[1]).max(0.0),
        };
        n.children = self.children.iter().map(ProbeNodeWire::to_a11y).collect();
        n
    }
}

/// Which root, if any, is a modal covering the others.
///
/// By geometry rather than by count: two roots of the same size are two
/// halves of a screen, not a dialog over one. A modal is strictly smaller
/// than something it sits on top of, and Compose composes it into its own
/// root — so the test is "is there exactly one root that every other root
/// strictly contains".
///
/// Returns `None` for the ordinary case of a single root, and for anything
/// ambiguous. Being wrong towards "no modal" leaves smix where it was
/// before this release; being wrong the other way makes half a screen
/// silently unaddressable.
fn modal_index(roots: &[A11yNode]) -> Option<usize> {
    if roots.len() < 2 {
        return None;
    }
    let candidates: Vec<usize> = (0..roots.len())
        .filter(|&i| {
            roots.iter().enumerate().all(|(j, other)| {
                j == i || strictly_contains(&other.bounds, &roots[i].bounds)
            })
        })
        .collect();
    // Exactly one, or none. There cannot be two: strict containment runs
    // one way, so X inside Y and Y inside X is impossible.
    //
    // A first draft guarded against two candidates anyway, and a mutation
    // sweep showed that guard could never be the reason for a verdict —
    // an unreachable branch is a predicate that will never go red, which
    // is the same emptiness as one that is always true. The same sweep
    // showed relaxing the strictness below changes no outcome either, for
    // the same reason: equal areas that contain each other are one
    // rectangle twice.
    match candidates.as_slice() {
        [only] => Some(*only),
        _ => None,
    }
}

fn strictly_contains(outer: &smix_screen::Rect, inner: &smix_screen::Rect) -> bool {
    let bigger = outer.w * outer.h > inner.w * inner.h;
    bigger
        && outer.x <= inner.x
        && outer.y <= inner.y
        && outer.x + outer.w >= inner.x + inner.w
        && outer.y + outer.h >= inner.y + inner.h
}

fn blank_node() -> A11yNode {
    serde_json::from_str(
        r#"{"rawType":"Other","bounds":{"x":0.0,"y":0.0,"w":0.0,"h":0.0},
            "enabled":true,"selected":false,"hasFocus":false,"visible":true,
            "children":[]}"#,
    )
    .expect("the blank node is well formed")
}

fn union_bounds(nodes: &[A11yNode]) -> smix_screen::Rect {
    let mut x0 = f64::MAX;
    let mut y0 = f64::MAX;
    let mut x1 = f64::MIN;
    let mut y1 = f64::MIN;
    for n in nodes {
        x0 = x0.min(n.bounds.x);
        y0 = y0.min(n.bounds.y);
        x1 = x1.max(n.bounds.x + n.bounds.w);
        y1 = y1.max(n.bounds.y + n.bounds.h);
    }
    if nodes.is_empty() {
        return smix_screen::Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
    }
    smix_screen::Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
}
