//! `smix tree`: read the runner's tree and print it.

use crate::act::ActError;
use smix_driver::SimctlDriver;
use smix_runner_client::HttpRunnerClient;

/// `smix tree [--json]` — print the runner's current accessibility tree.
/// `--json` emits the wire-format JSON (large — typically 100KB+ for a
/// typical app screen); default emits an indented text outline keyed by
/// id + label per node.
/// Which reader `smix tree` asks. See the CLI's own enum for why a
/// caller would name one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeReader {
    Auto,
    Probe,
    A11y,
}

pub async fn cmd_tree(
    json: bool,
    port: u16,
    keyboard: bool,
    reader: TreeReader,
    ios: bool,
) -> Result<(), ActError> {
    // The client rather than the driver, because the driver's `tree` hands
    // back the root alone and the source is the half a reader needs most
    // when the answer looks thin. A screen the accessibility reader has
    // gone blind on and a screen with nothing on it print identically
    // otherwise.
    let client = HttpRunnerClient::new(port);
    let mut perceived = match reader {
        TreeReader::Auto => client.get_tree(None).await,
        TreeReader::Probe => client.semantics_tree_only().await,
        TreeReader::A11y => client.accessibility_tree_only(None).await,
    }
    .map_err(|e| ActError::Transport(format!("{e}")))?;
    if !keyboard {
        collapse_keyboards(&mut perceived.root);
    }
    if json {
        let s = serde_json::to_string_pretty(&perceived)
            .map_err(|e| ActError::Transport(format!("serde: {e}")))?;
        println!("{s}");
    } else {
        // The accessibility reader's blind spot is a Compose dialog, and
        // the way out is an Android dependency: on iOS the note is noise.
        if !ios && let Some(caveat) = perceived.caveat() {
            println!("# {caveat}");
        }
        print_tree_outline(&perceived.root, 0);
    }
    Ok(())
}

/// Drop the keys under every keyboard, recording how many there were.
///
/// The keyboard node stays, and the outline prints the count next to
/// it, so the reader is told what was left out and can ask for it with
/// `--keyboard`. In `--json` the keys are simply absent — the keyboard
/// node's presence is the signal there.
pub(crate) fn collapse_keyboards(node: &mut smix_screen::A11yNode) -> usize {
    if smix_screen::is_keyboard(node) {
        let keys = smix_screen::subtree_len(node) - 1;
        node.children.clear();
        return keys;
    }
    node.children.iter_mut().map(collapse_keyboards).sum()
}

/// Helper for authoring subcommand to fetch the a11y
/// tree as raw JSON (bypasses print_tree_outline).
pub async fn fetch_tree_json(port: u16) -> Result<serde_json::Value, ActError> {
    let d = SimctlDriver::new(HttpRunnerClient::new(port));
    let tree = d
        .tree(None)
        .await
        .map_err(|e| ActError::Transport(format!("{e}")))?;
    serde_json::to_value(&tree).map_err(|e| ActError::Transport(format!("serialize tree: {e}")))
}

pub(crate) fn outline_line(node: &smix_screen::A11yNode, depth: usize) -> String {
    let indent = "  ".repeat(depth);
    let id = node.identifier.as_deref().unwrap_or("");
    let label = node.label.as_deref().unwrap_or("");
    let text = node.text.as_deref().unwrap_or("");
    let visible = if node.visible { "✓" } else { "·" };
    let note = if smix_screen::is_keyboard(node) && node.children.is_empty() {
        "  (keys collapsed — --keyboard to include them)"
    } else {
        ""
    };
    // Print text only when it carries something. iOS puts its semantics in
    // label/value/title and leaves text empty; Android puts it in text with
    // label often empty. Appending unconditionally would fill iOS output
    // with `text=""` noise; the guard keeps iOS unchanged and surfaces
    // Android's text (the ⑤ that made SUBMIT invisible in the human tree).
    let text_field = if text.is_empty() {
        String::new()
    } else {
        format!(" text={text:?}")
    };
    format!("{indent}{visible} id={id:?} label={label:?}{text_field}{note}")
}

pub(crate) fn print_tree_outline(node: &smix_screen::A11yNode, depth: usize) {
    println!("{}", outline_line(node, depth));
    for child in &node.children {
        print_tree_outline(child, depth + 1);
    }
}
