//! What a selector matches that is in the tree but not on screen.

use crate::ResolverContext;
use smix_screen::{A11yNode, is_visible_enough};
use smix_selector::Selector;

/// The nodes `selector` names that the tree holds off screen.
///
/// Resolution drops these before anything else, which is right for
/// acting on them and wrong for explaining a miss: "matched 0 nodes"
/// sends the reader to check the selector, when the element is there and
/// only has to be scrolled to. A `fallback:` chain answers for its first
/// layer that has any.
#[must_use]
pub fn matched_off_screen<'tree>(
    tree: &'tree A11yNode,
    selector: &Selector,
) -> Vec<&'tree A11yNode> {
    let Some(ctx) = ResolverContext::new(selector) else {
        return vec![];
    };
    off_screen(tree, selector, &ctx)
}

fn off_screen<'tree>(
    tree: &'tree A11yNode,
    selector: &Selector,
    ctx: &ResolverContext,
) -> Vec<&'tree A11yNode> {
    if let Selector::Fallback { fallback } = selector {
        return fallback
            .iter()
            .map(|layer| off_screen(tree, layer, ctx))
            .find(|found| !found.is_empty())
            .unwrap_or_default();
    }
    crate::collect::candidates(tree, selector, ctx)
        .into_iter()
        .filter(|n| !is_visible_enough(n, tree))
        .collect()
}
