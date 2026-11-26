//! The probe's semantics payload, read into the tree everything else uses.

use crate::{
    A11yNode, BULLET, Rect, Role, blank_node, carries_anything, modal_index, union_bounds,
};

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
    // Two shapes: a bare array of roots, and an envelope carrying the
    // screen's size beside them. The envelope is what the runner sends
    // since the tap arithmetic needed a denominator; the bare form is
    // still accepted because the probe's own `tree` method answers that
    // way and a gate reads it directly.
    let (screen, roots): (Option<[f64; 2]>, Vec<ProbeNodeWire>) =
        match serde_json::from_str::<ProbeEnvelope>(json) {
            Ok(e) => (Some(e.screen), e.roots),
            Err(_) => (None, serde_json::from_str(json).ok()?),
        };
    let mut converted: Vec<A11yNode> = roots.iter().map(ProbeNodeWire::to_a11y).collect();
    // Nothing to say yet is absence, not an empty screen.
    //
    // For a moment after the app restarts the probe answers `[]` — its
    // process is new and no Compose root has registered. Converting that
    // into a tree let it SHADOW the accessibility tree, and a fill that had
    // always worked reported ELEMENT_NOT_FOUND against a field plainly on
    // screen. Caught by running the flow it was written for.
    if converted.iter().all(|r| !carries_anything(r)) {
        return None;
    }
    // The probe says which windows take every touch, and lists them in the
    // order they stand; the topmost such window hides what stands below it
    // from reach. A probe too old to say is judged by the roots' shapes, as
    // it always was.
    if roots.iter().any(|r| r.modal.is_some()) {
        let front = roots
            .iter()
            .rposition(|r| r.modal == Some(true))
            .unwrap_or(0);
        converted.drain(..front);
    } else if let Some(modal) = modal_index(&converted) {
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
    // The SCREEN, not the union of the roots.
    //
    // Everything downstream normalises a node's rectangle against the tree
    // root's, and Compose occupies only part of the display — so a union
    // here put a tap a fifth of a screen off, and `/input-text` answered
    // `no_focused_field`: a message about the keyboard for a fault in the
    // arithmetic. Falls back to the union when the probe is too old to
    // report a size, which is wrong by the same amount but no worse than
    // before it was asked.
    parent.bounds = match screen {
        Some([w, h]) if w > 0.0 && h > 0.0 => Rect {
            x: 0.0,
            y: 0.0,
            w,
            h,
        },
        _ => union_bounds(&converted),
    };
    parent.children = converted;
    Some(parent)
}

#[derive(serde::Deserialize)]
struct ProbeEnvelope {
    screen: [f64; 2],
    roots: Vec<ProbeNodeWire>,
}

#[derive(serde::Deserialize)]
struct ProbeNodeWire {
    #[serde(default)]
    #[allow(dead_code)]
    id: i64,
    #[serde(default, rename = "testTag")]
    test_tag: Option<String>,
    /// A hosted View's own id. A Compose node has a `testTag` and a View
    /// has this; one selector (`id:`) addresses both, because from a flow
    /// they are the same question.
    #[serde(default, rename = "resourceId")]
    resource_id: Option<String>,
    #[serde(default)]
    text: Option<String>,
    /// A hosted View's hint — what an empty field shows. Carried as the
    /// node's placeholder, the way the accessibility reader carries it,
    /// never as its text.
    #[serde(default)]
    hint: Option<String>,
    #[serde(default, rename = "editableText")]
    editable_text: Option<String>,
    /// What a field actually holds. Separate from `editableText`, which on
    /// a masked field reads back as bullets — Compose applies the visual
    /// transformation before semantics sees it.
    #[serde(default, rename = "inputText")]
    input_text: Option<String>,
    #[serde(default, rename = "contentDescription")]
    content_description: Option<String>,
    /// The role, already spelled the way the accessibility wire spells it
    /// — the runner fills it from the class using the one table that maps
    /// them, rather than a second copy of it living here.
    ///
    /// A string rather than `Role`, and converted leniently below: a
    /// spelling this build does not know must cost that node its role, not
    /// cost the caller the whole tree.
    #[serde(default)]
    role: Option<String>,
    /// The hosted View's class. Carried for the same reason the
    /// accessibility path carries it: it is the fact, where the role is a
    /// reading of it.
    #[serde(default, rename = "className")]
    class_name: Option<String>,
    /// Whether any of the node shows. Absent from probes older than this
    /// field, and absence is not a "no" — see the default.
    #[serde(default = "yes")]
    visible: bool,
    #[serde(default)]
    bounds: [f64; 4],
    /// The part of `bounds` that shows. Absent from probes older than
    /// this field, and absent is not "all of it" — the host carries the
    /// absence through rather than answering for a reader that did not.
    #[serde(default, rename = "visibleBounds")]
    visible_bounds: Option<[f64; 4]>,
    /// Where a `TouchDelegate` sends touches to this View; the runner adds
    /// it when asked, the probe itself never does.
    #[serde(default, rename = "touchBounds")]
    touch_bounds: Option<[f64; 4]>,
    #[serde(default)]
    focused: bool,
    #[serde(default = "yes")]
    enabled: bool,
    #[serde(default)]
    children: Vec<ProbeNodeWire>,
    /// On a window's root: whether that window takes every touch on the
    /// screen. Absent from a probe older than the field.
    #[serde(default)]
    modal: Option<bool>,
}

fn yes() -> bool {
    true
}

impl ProbeNodeWire {
    fn to_a11y(&self) -> A11yNode {
        let mut n = blank_node();
        // A Compose node names itself with a testTag and a hosted View
        // with its resource id. Never both, and `id:` in a flow means
        // whichever this node has.
        n.identifier = self.test_tag.clone().or_else(|| self.resource_id.clone());
        n.label = self.content_description.clone();
        n.text = self.text.clone();
        n.placeholder_value = self.hint.clone();
        // `inputText` first: it is what was typed, where `editableText` is
        // what is shown. A predicate comparing a masked field with what a
        // flow typed asks a question only the first can answer.
        n.value = self
            .input_text
            .clone()
            .or_else(|| self.editable_text.clone());
        n.enabled = self.enabled;
        n.has_focus = self.focused;
        n.visible = self.visible;
        if let Some(c) = &self.class_name {
            n.raw_type = c.clone();
        }
        n.role = self
            .role
            .as_deref()
            .and_then(|r| serde_json::from_value(serde_json::Value::String(r.to_string())).ok());
        n.bounds = Rect {
            x: self.bounds[0],
            y: self.bounds[1],
            w: (self.bounds[2] - self.bounds[0]).max(0.0),
            h: (self.bounds[3] - self.bounds[1]).max(0.0),
        };
        let rect = |b: [f64; 4]| Rect {
            x: b[0],
            y: b[1],
            w: (b[2] - b[0]).max(0.0),
            h: (b[3] - b[1]).max(0.0),
        };
        n.visible_bounds = self.visible_bounds.map(rect);
        n.touch_bounds = self.touch_bounds.map(rect);
        // A masked field shows one bullet per character as its editable
        // text, while `inputText` carries what was typed. Said by its
        // role, so everything that writes a node out can leave it out.
        if self
            .editable_text
            .as_deref()
            .is_some_and(|t| !t.is_empty() && t.chars().all(|c| c == BULLET))
            && matches!(n.role, None | Some(Role::TextField))
        {
            n.role = Some(Role::SecureTextField);
        }
        n.children = self.children.iter().map(ProbeNodeWire::to_a11y).collect();
        n
    }
}
