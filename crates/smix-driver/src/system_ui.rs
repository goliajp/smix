//! A touch into a notification SystemUI has just shown.
//!
//! A heads-up notification does not take a touch while it slides in.
//! Measured on an Android 16 emulator, tapping its title at a fixed delay
//! after it first appeared in the tree: 0 ms and 150 ms, 0 of 4 opened the
//! app; 300 ms, 1 of 2; 500 ms and 800 ms, 4 of 4. The dropped tap leaves
//! the notification where it was, and the step after it fails on an app
//! that never came forward.

use std::time::{Duration, Instant};

use smix_screen::A11yNode;
use smix_selector::Selector;
use smix_selector_resolver::resolve_selector;

/// How long a target in SystemUI's windows has to have been on screen
/// before it is touched: the 500 ms that always worked, and some margin
/// for a loaded machine.
pub(crate) const SETTLES_WITHIN: Duration = Duration::from_millis(700);

const SYSTEM_UI: &str = "com.android.systemui";

/// When this step first saw its target in a SystemUI window — never
/// earlier than the window really appeared, so waiting from here is
/// enough.
#[derive(Default)]
pub(crate) struct Entering(Option<Instant>);

impl Entering {
    /// How much longer to wait before touching `selector`'s target, or
    /// `None` when it is not in SystemUI or has been on screen long enough.
    pub(crate) fn wait(&mut self, tree: &A11yNode, selector: &Selector) -> Option<Duration> {
        if !in_system_ui(tree, selector) {
            return None;
        }
        let first = *self.0.get_or_insert_with(Instant::now);
        SETTLES_WITHIN
            .checked_sub(first.elapsed())
            .filter(|d| !d.is_zero())
    }
}

fn in_system_ui(tree: &A11yNode, selector: &Selector) -> bool {
    tree.children.iter().any(|w| {
        w.window.as_ref().and_then(|i| i.package.as_deref()) == Some(SYSTEM_UI)
            && resolve_selector(w, selector).is_some()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use smix_screen::{WindowInfo, WindowKind};

    fn node(json: &str) -> A11yNode {
        serde_json::from_str(json).unwrap()
    }

    fn window(pkg: &str, text: &str) -> A11yNode {
        let mut w = node(&format!(
            r#"{{"rawType":"x","elementTypeRaw":0,"enabled":true,"selected":false,"hasFocus":false,"visible":true,
            "bounds":{{"x":0,"y":0,"w":100,"h":100}},
            "children":[{{"rawType":"android.widget.TextView","elementTypeRaw":0,"enabled":true,"selected":false,
            "hasFocus":false,"visible":true,"text":"{text}","bounds":{{"x":10,"y":10,"w":50,"h":20}}}}]}}"#
        ));
        w.window = Some(WindowInfo {
            package: Some(pkg.into()),
            kind: WindowKind::System,
            focused: false,
            layer: None,
            touchable: None,
            touchable_rects: vec![],
        });
        w
    }

    fn screen(windows: Vec<A11yNode>) -> A11yNode {
        let mut root = node(
            r#"{"rawType":"x","elementTypeRaw":0,"enabled":true,"selected":false,"hasFocus":false,"visible":true,"bounds":{"x":0,"y":0,"w":100,"h":100}}"#,
        );
        root.children = windows;
        root
    }

    fn title() -> Selector {
        Selector::Text {
            text: smix_selector::Pattern::text("Order shipped"),
            modifiers: smix_selector::Modifiers::default(),
        }
    }

    #[test]
    fn a_target_in_system_ui_waits_from_when_it_was_first_seen() {
        let t = screen(vec![window(SYSTEM_UI, "Order shipped")]);
        let sel = title();
        let mut e = Entering::default();
        assert!(e.wait(&t, &sel).is_some_and(|d| d <= SETTLES_WITHIN));
        e.0 = Some(Instant::now() - SETTLES_WITHIN);
        assert_eq!(e.wait(&t, &sel), None, "on screen long enough");
    }

    #[test]
    fn a_target_in_the_app_does_not_wait() {
        let t = screen(vec![window("com.example.shop", "Order shipped")]);
        assert_eq!(Entering::default().wait(&t, &title()), None);
    }
}
