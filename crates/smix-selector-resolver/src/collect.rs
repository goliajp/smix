//! Which nodes a selector's base matches, before any filter runs.

use smix_screen::{A11yNode, is_keyboard};
use smix_selector::Selector;

use crate::{ResolverContext, matches_base};

/// Every node the selector's base matches, in tree order.
///
/// `focused` asks for the field that input goes to, and the keyboard
/// is what input comes from: a node of the keyboard's own window that
/// reports focus is never that field. So a keyboard's subtree is not
/// searched for it. Tapping such a node's centre pressed a key.
pub(crate) fn candidates<'tree>(
    tree: &'tree A11yNode,
    selector: &Selector,
    ctx: &ResolverContext,
) -> Vec<&'tree A11yNode> {
    let skip_keyboard = matches!(selector, Selector::Focused { .. });
    let mut out = Vec::new();
    walk(tree, &mut out, &|n: &A11yNode| {
        if skip_keyboard && is_keyboard(n) {
            Visit::SkipSubtree
        } else if matches_base(n, selector, ctx) {
            Visit::Take
        } else {
            Visit::Descend
        }
    });
    out
}

enum Visit {
    Take,
    Descend,
    SkipSubtree,
}

fn walk<'tree>(
    n: &'tree A11yNode,
    out: &mut Vec<&'tree A11yNode>,
    visit: &dyn Fn(&A11yNode) -> Visit,
) {
    match visit(n) {
        Visit::SkipSubtree => return,
        Visit::Take => out.push(n),
        Visit::Descend => {}
    }
    for c in &n.children {
        walk(c, out, visit);
    }
}
