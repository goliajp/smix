//! The elements a reader could name, in reading order, each once.

use crate::{
    A11yNode, ElementSummary, Rect, has_identity, in_window_order, is_keyboard, is_visible_enough,
    summarize_node,
};

/// [`collect_visible_summaries`], with the keyboard optional.
///
/// The keyboard is one node saying "keyboard" and then a key per
/// letter, plus `Next keyboard`, `Dictate`, `shift`, `delete` and the
/// rest — around sixty summaries that are the same sixty on every
/// screen in every app. A description of a login form spent most of
/// itself on them, and a description is read by an AI paying for every
/// token of it.
///
/// So they are left out by default and the caller can ask. What is not
/// left out is *that a keyboard is up*: the keyboard node itself is
/// still summarised, because a keyboard covering the thing you wanted
/// to tap is the explanation for a failure, and hiding it would turn a
/// legible failure into a mystery. Only the keys go.
#[must_use]
pub fn collect_visible_summaries_with(
    tree: &A11yNode,
    limit: usize,
    include_keyboard_keys: bool,
) -> Vec<ElementSummary> {
    let mut out: Vec<ElementSummary> = Vec::new();
    fn walk(
        n: &A11yNode,
        root: &A11yNode,
        limit: usize,
        keys: bool,
        owner: Option<&ElementSummary>,
        out: &mut Vec<ElementSummary>,
    ) {
        if out.len() >= limit {
            return;
        }
        let mut listed = None;
        if n.enabled && n.visible && is_visible_enough(n, root) {
            let s = summarize_node(n);
            if has_identity(&s) && !repeats(&s, owner, out.last()) {
                out.push(s.clone());
                listed = Some(s);
            }
        }
        if !keys && is_keyboard(n) {
            return;
        }
        let owner = listed.as_ref().or(owner);
        for c in &n.children {
            if out.len() >= limit {
                return;
            }
            walk(c, root, limit, keys, owner, out);
        }
    }
    // The root is read first as before; its children are read in window
    // order. Only the root's children: that is where a reader that serves
    // several windows puts them, and reordering deeper would change the
    // order of the app's own elements, which is the order a reader of the
    // screen expects.
    if tree.enabled && tree.visible && is_visible_enough(tree, tree) {
        let s = summarize_node(tree);
        if has_identity(&s) && out.len() < limit {
            out.push(s);
        }
    }
    if include_keyboard_keys || !is_keyboard(tree) {
        // the root is the app, not a control: its children are not its title
        for c in in_window_order(&tree.children) {
            if out.len() >= limit {
                break;
            }
            walk(c, tree, limit, include_keyboard_keys, None, &mut out);
        }
    }
    out
}

/// Is `s` already on the list?
///
/// Two ways a tree says one thing twice. A button and the label inside
/// it carry the same name — "Light", then "Light" again inside its frame
/// — and the label adds nothing a reader can act on that the button does
/// not. And a reader can report the same element twice in a row, as iOS
/// does with a scroll bar. Anything with its own id is kept: an id is
/// something to select by.
fn repeats(
    s: &ElementSummary,
    owner: Option<&ElementSummary>,
    last: Option<&ElementSummary>,
) -> bool {
    if last == Some(s) {
        return true;
    }
    let Some(owner) = owner else {
        return false;
    };
    s.id.is_none() && s.name.is_some() && s.name == owner.name && inside(&s.bounds, &owner.bounds)
}

fn inside(a: &Rect, b: &Rect) -> bool {
    a.x >= b.x - 0.5
        && a.y >= b.y - 0.5
        && a.x + a.w <= b.x + b.w + 0.5
        && a.y + a.h <= b.y + b.h + 0.5
}

impl crate::ScreenDescription {
    /// One line per element — its role, its name, its id — and nothing
    /// else: no bounds, no flags. What a reader needs to write the next
    /// selector, at a fraction of the size of the full description.
    #[must_use]
    pub fn compact(&self) -> String {
        let mut out = String::new();
        if let Some(app) = &self.front_app {
            out.push_str(&format!("app {app}\n"));
        }
        for e in &self.elements {
            let role = e
                .role
                .and_then(|r| serde_json::to_value(r).ok())
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_else(|| "-".to_string());
            out.push_str(&role);
            if let Some(name) = e.name.as_ref().or(e.text.as_ref()) {
                out.push_str(&format!(" {name:?}"));
            }
            if let Some(id) = &e.id {
                out.push_str(&format!(" #{id}"));
            }
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ScreenDescription;

    fn node(
        raw: &str,
        label: Option<&str>,
        id: Option<&str>,
        b: (f64, f64, f64, f64),
        children: Vec<A11yNode>,
    ) -> A11yNode {
        let mut n: A11yNode = serde_json::from_value(serde_json::json!({
            "rawType": raw, "enabled": true, "selected": false, "hasFocus": false, "visible": true,
            "bounds": {"x": b.0, "y": b.1, "w": b.2, "h": b.3}
        }))
        .expect("a node");
        n.label = label.map(str::to_string);
        n.role = serde_json::from_value(serde_json::json!(raw)).ok();
        n.identifier = id.map(str::to_string);
        n.children = children;
        n
    }

    /// The consumer's profile screen in small: a button and the label
    /// inside it, and a scroll bar the reader reported twice.
    fn profile() -> A11yNode {
        let bar = || {
            node(
                "other",
                Some("Horizontal scroll bar, 1 page"),
                None,
                (0.0, 696.0, 402.0, 30.0),
                vec![],
            )
        };
        node(
            "application",
            Some("Insight"),
            Some("com.example.app"),
            (0.0, 0.0, 402.0, 874.0),
            vec![
                node(
                    "staticText",
                    Some("Insight"),
                    None,
                    (150.0, 60.0, 80.0, 20.0),
                    vec![],
                ),
                node(
                    "button",
                    Some("Light"),
                    Some("theme-light"),
                    (21.0, 152.0, 121.0, 45.0),
                    vec![node(
                        "staticText",
                        Some("Light"),
                        None,
                        (65.0, 166.0, 32.0, 16.0),
                        vec![],
                    )],
                ),
                bar(),
                bar(),
                node(
                    "staticText",
                    Some("Light"),
                    None,
                    (21.0, 300.0, 40.0, 16.0),
                    vec![],
                ),
            ],
        )
    }

    fn names(tree: &A11yNode) -> Vec<String> {
        collect_visible_summaries_with(tree, usize::MAX, false)
            .into_iter()
            .map(|e| e.name.unwrap_or_default())
            .collect()
    }

    #[test]
    fn a_button_and_the_label_inside_it_are_listed_once() {
        let n = names(&profile());
        assert_eq!(n.iter().filter(|s| *s == "Light").count(), 2, "{n:?}");
    }

    #[test]
    fn the_same_element_reported_twice_in_a_row_is_listed_once() {
        let n = names(&profile());
        assert_eq!(
            n.iter().filter(|s| s.starts_with("Horizontal")).count(),
            1,
            "{n:?}"
        );
    }

    #[test]
    fn a_title_that_happens_to_be_the_app_name_is_kept() {
        let n = names(&profile());
        assert_eq!(n.iter().filter(|s| *s == "Insight").count(), 2, "{n:?}");
    }

    #[test]
    fn compact_is_role_name_and_id_and_nothing_else() {
        let d = ScreenDescription {
            screenshot: None,
            elements: collect_visible_summaries_with(&profile(), usize::MAX, false),
            front_app: Some("com.example.app".into()),
            summary: String::new(),
            captured_at: 0.0,
        };
        let c = d.compact();
        assert!(c.contains("button \"Light\" #theme-light\n"), "{c}");
        assert!(!c.contains("152"), "no bounds: {c}");
        assert!(c.lines().count() < serde_json::to_string_pretty(&d).unwrap().lines().count() / 4);
    }
}
