//! What a failure says about the screen it happened on.

use crate::{A11yNode, ElementSummary, WindowInfo, collect_visible_summaries, in_window_order};

/// What a failure says about the screen it happened on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScreenFacts {
    /// The first `limit` elements a reader could name, the focused app's
    /// first, and after them every one that carries words — a reader
    /// looking for text the selector missed needs all of the screen's
    /// text, not the first ten elements of it.
    pub elements: Vec<ElementSummary>,
    /// How many there were before the list was cut.
    pub total: usize,
    /// The windows on screen, focused application first.
    pub windows: Vec<WindowInfo>,
    /// Windows the reader could not read.
    pub unreadable: u32,
}

/// The screen, as a failure should describe it.
#[must_use]
pub fn screen_facts(tree: &A11yNode, limit: usize) -> ScreenFacts {
    let all = collect_visible_summaries(tree, usize::MAX);
    let mut windows: Vec<WindowInfo> = tree.window.iter().cloned().collect();
    windows.extend(
        in_window_order(&tree.children)
            .into_iter()
            .filter_map(|c| c.window.clone()),
    );
    ScreenFacts {
        total: all.len(),
        elements: all
            .into_iter()
            .enumerate()
            .filter(|(i, e)| *i < limit || carries_words(e))
            .map(|(_, e)| e)
            .collect(),
        windows,
        unreadable: tree.unreadable_windows.unwrap_or(0),
    }
}

pub(crate) fn carries_words(e: &ElementSummary) -> bool {
    let said = |s: &Option<String>| s.as_deref().is_some_and(|s| !s.trim().is_empty());
    said(&e.name) || said(&e.text)
}
