//! `smix tap` / `smix find` / `smix wait-for` CLI subcommands.
//!
//! Shell-out act/sense surface on top of a running runner (see
//! `smix capsule up` / `smix runner up`). The CLI plumbs through to
//! `smix-runner-client` via HTTP at `localhost:<port>`. Port defaults to
//! 22087 (single-sim) and reads `SMIX_RUNNER_PORT` env when set
//! (used to bind multiple concurrent runners to distinct ports).
//!
//! Selector shorthand (parsed once at CLI parse-time):
//!   - `id:btn-take-photo` → Selector::Id { id: "btn-take-photo", ... }
//!   - `text:Welcome to smix` → Selector::Text { text: Pattern::Text(..), ... }
//!   - `label:Settings` → Selector::Label { label: "Settings", ... }
//!   - `role:button` → Selector::Role { role: Role::Button, ... }
//!
//! Examples:
//!   $ smix capsule up ios-17 --soft --no-capture
//!   $ smix find 'id:home-tab'
//!   exists=true
//!   $ smix tap 'id:home-tab'
//!   tapped id=home-tab
//!   $ smix wait-for 'id:home-counter-label' --timeout 5
//!   visible id=home-counter-label (waited 12ms)

use smix_driver::SimctlDriver;
use smix_input::{KeyName, SwipeDirection};
use smix_runner_client::HttpRunnerClient;
use smix_screen::role_from_raw_type;
use smix_selector::{Modifiers, Pattern, Selector};
use std::time::Duration;

const DEFAULT_RUNNER_PORT: u16 = 22087;

/// Read SMIX_RUNNER_PORT env or fall back to 22087 default.
pub fn runner_port_from_env() -> u16 {
    std::env::var("SMIX_RUNNER_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(DEFAULT_RUNNER_PORT)
}

/// Parse `<kind>:<value>` selector shorthand. Returns None on unknown kind
/// / missing colon so the CLI can surface a clear "selector parse error".
pub fn parse_selector(s: &str) -> Option<Selector> {
    let (kind, value) = s.split_once(':')?;
    let modifiers = Modifiers::default();
    match kind {
        "id" => Some(Selector::Id {
            id: value.to_string(),
            modifiers,
        }),
        "text" => Some(Selector::Text {
            text: Pattern::text(value),
            modifiers,
        }),
        "label" => Some(Selector::Label {
            label: value.to_string(),
            modifiers,
        }),
        "role" => {
            let role = role_from_raw_type(value)?;
            Some(Selector::Role {
                role,
                name: None,
                modifiers,
            })
        }
        _ => None,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ActError {
    #[error("invalid selector `{0}` — expected one of `id:` / `text:` / `label:` / `role:`")]
    BadSelector(String),
    #[error("runner transport: {0}")]
    Transport(String),
    #[error("wait_for timeout after {timeout_ms}ms: {selector}")]
    Timeout { selector: String, timeout_ms: u64 },
}

fn driver(port: u16) -> SimctlDriver {
    SimctlDriver::new(HttpRunnerClient::new(port))
}

/// Parse a KeyName shorthand mirroring the wire camelCase form
/// `smix_input::KeyName::as_str` produces. Accepts a couple of common
/// shell-friendly aliases (`enter` → return, `backspace` → delete).
pub fn parse_key_name(s: &str) -> Option<KeyName> {
    match s {
        "return" | "enter" => Some(KeyName::Return),
        "delete" | "backspace" => Some(KeyName::Delete),
        "tab" => Some(KeyName::Tab),
        "space" => Some(KeyName::Space),
        "escape" | "esc" => Some(KeyName::Escape),
        "arrowUp" | "up" => Some(KeyName::ArrowUp),
        "arrowDown" | "down" => Some(KeyName::ArrowDown),
        "arrowLeft" | "left" => Some(KeyName::ArrowLeft),
        "arrowRight" | "right" => Some(KeyName::ArrowRight),
        "home" => Some(KeyName::Home),
        "lock" => Some(KeyName::Lock),
        "volumeUp" | "volume-up" => Some(KeyName::VolumeUp),
        "volumeDown" | "volume-down" => Some(KeyName::VolumeDown),
        _ => None,
    }
}

/// Parse swipe / scroll direction.
pub fn parse_direction(s: &str) -> Option<SwipeDirection> {
    match s {
        "up" => Some(SwipeDirection::Up),
        "down" => Some(SwipeDirection::Down),
        "left" => Some(SwipeDirection::Left),
        "right" => Some(SwipeDirection::Right),
        _ => None,
    }
}

/// `smix tap <selector>` — host-resolve + tap_at_norm_coord on the running
/// runner. Routes through `SimctlDriver::tap` so id/label/role selectors
/// resolve via the /tree path (swift /tap only supports text selectors).
pub async fn cmd_tap(selector_str: String, port: u16) -> Result<(), ActError> {
    let selector =
        parse_selector(&selector_str).ok_or_else(|| ActError::BadSelector(selector_str.clone()))?;
    let d = driver(port);
    d.tap(&selector, None)
        .await
        .map_err(|e| ActError::Transport(format!("{e}")))?;
    println!("tapped {selector_str}");
    Ok(())
}

/// `smix find <selector>` — boolean existence probe. Same routing path as
/// `smix tap`: text → swift /find shortcut, anything else → /tree resolve.
pub async fn cmd_find(selector_str: String, port: u16) -> Result<(), ActError> {
    let selector =
        parse_selector(&selector_str).ok_or_else(|| ActError::BadSelector(selector_str.clone()))?;
    let d = driver(port);
    let exists = d
        .find(&selector, None)
        .await
        .map_err(|e| ActError::Transport(format!("{e}")))?;
    println!("exists={exists}");
    Ok(())
}

/// `smix fill <selector> --text <text>` — type `text` into the matched field.
/// Mirrors maestro `inputText:`.
pub async fn cmd_fill(selector_str: String, text: String, port: u16) -> Result<(), ActError> {
    let selector =
        parse_selector(&selector_str).ok_or_else(|| ActError::BadSelector(selector_str.clone()))?;
    let d = driver(port);
    d.fill(&selector, &text, None)
        .await
        .map_err(|e| ActError::Transport(format!("{e}")))?;
    println!("filled {selector_str} with `{text}`");
    Ok(())
}

/// `smix press-key <key-name>` — issue a hardware / IME key press. Key
/// shorthand: `return` (alias `enter`), `delete` (alias `backspace`),
/// `tab`, `space`, `escape` / `esc`, `arrowUp` / `up`, `arrowDown` /
/// `down`, `arrowLeft` / `left`, `arrowRight` / `right`, `home`, `lock`,
/// `volumeUp` / `volume-up`, `volumeDown` / `volume-down`.
pub async fn cmd_press_key(key_str: String, port: u16) -> Result<(), ActError> {
    let key =
        parse_key_name(&key_str).ok_or_else(|| ActError::BadSelector(format!("key:{key_str}")))?;
    let d = driver(port);
    d.press_key(key)
        .await
        .map_err(|e| ActError::Transport(format!("{e}")))?;
    println!("pressed key:{key_str}");
    Ok(())
}

/// `smix scroll <selector> --direction <up|down|left|right>` — scroll
/// until selector becomes visible.
pub async fn cmd_scroll(
    selector_str: String,
    direction_str: String,
    port: u16,
) -> Result<(), ActError> {
    let selector =
        parse_selector(&selector_str).ok_or_else(|| ActError::BadSelector(selector_str.clone()))?;
    let direction = parse_direction(&direction_str)
        .ok_or_else(|| ActError::BadSelector(format!("direction:{direction_str}")))?;
    let d = driver(port);
    d.scroll(&selector, direction)
        .await
        .map_err(|e| ActError::Transport(format!("{e}")))?;
    println!("scrolled {direction_str} to {selector_str}");
    Ok(())
}

/// `smix tree [--json]` — print the runner's current accessibility tree.
/// `--json` emits the wire-format JSON (large — typically 100KB+ for a
/// typical app screen); default emits an indented text outline keyed by
/// id + label per node.
pub async fn cmd_tree(json: bool, port: u16) -> Result<(), ActError> {
    let d = driver(port);
    let tree = d
        .tree(None)
        .await
        .map_err(|e| ActError::Transport(format!("{e}")))?;
    if json {
        let s = serde_json::to_string_pretty(&tree)
            .map_err(|e| ActError::Transport(format!("serde: {e}")))?;
        println!("{s}");
    } else {
        print_tree_outline(&tree, 0);
    }
    Ok(())
}

/// Helper for authoring subcommand to fetch the a11y
/// tree as raw JSON (bypasses print_tree_outline).
pub async fn fetch_tree_json(port: u16) -> Result<serde_json::Value, ActError> {
    let d = driver(port);
    let tree = d
        .tree(None)
        .await
        .map_err(|e| ActError::Transport(format!("{e}")))?;
    serde_json::to_value(&tree).map_err(|e| ActError::Transport(format!("serialize tree: {e}")))
}

fn print_tree_outline(node: &smix_screen::A11yNode, depth: usize) {
    let indent = "  ".repeat(depth);
    let id = node.identifier.as_deref().unwrap_or("");
    let label = node.label.as_deref().unwrap_or("");
    let visible = if node.visible { "✓" } else { "·" };
    println!("{indent}{visible} id={id:?} label={label:?}");
    for child in &node.children {
        print_tree_outline(child, depth + 1);
    }
}

/// `smix describe [--json]` — print the runner's high-level ScreenDescription
/// (title / interactive elements / status bar / etc.). `--json` emits the
/// wire JSON; default emits a pretty-printed Debug summary.
pub async fn cmd_describe(json: bool, port: u16) -> Result<(), ActError> {
    let d = driver(port);
    let desc = d
        .describe()
        .await
        .map_err(|e| ActError::Transport(format!("{e}")))?;
    if json {
        let s = serde_json::to_string_pretty(&desc)
            .map_err(|e| ActError::Transport(format!("serde: {e}")))?;
        println!("{s}");
    } else {
        println!("{desc:#?}");
    }
    Ok(())
}

/// `smix system-popups [--json]` — print the runner's current SpringBoard
/// system-popup list (camera permission alerts, "Open in `<App>`?", etc.).
/// `--json` emits the wire JSON; default emits a pretty-printed Debug
/// summary keyed by popup id + buttons.
pub async fn cmd_system_popups(json: bool, port: u16) -> Result<(), ActError> {
    let d = driver(port);
    let popups = d
        .system_popups(None)
        .await
        .map_err(|e| ActError::Transport(format!("{e}")))?;
    if json {
        let s = serde_json::to_string_pretty(&popups)
            .map_err(|e| ActError::Transport(format!("serde: {e}")))?;
        println!("{s}");
    } else {
        if popups.is_empty() {
            println!("(no system popups in scope)");
        } else {
            println!("{popups:#?}");
        }
    }
    Ok(())
}

/// `smix hide-keyboard` — dismiss the soft keyboard if visible.
pub async fn cmd_hide_keyboard(port: u16) -> Result<(), ActError> {
    let d = driver(port);
    d.hide_keyboard()
        .await
        .map_err(|e| ActError::Transport(format!("{e}")))?;
    println!("keyboard hidden");
    Ok(())
}

/// `smix wait-for <selector> --timeout <secs>` — re-uses `SimctlDriver::
/// wait_for` (same polling + transient-transport-retry semantics the SDK
/// path uses). Returns visible-elements snapshot on timeout.
pub async fn cmd_wait_for(
    selector_str: String,
    timeout_secs: u64,
    port: u16,
) -> Result<(), ActError> {
    let selector =
        parse_selector(&selector_str).ok_or_else(|| ActError::BadSelector(selector_str.clone()))?;
    let d = driver(port);
    let timeout = Duration::from_secs(timeout_secs);
    match d.wait_for(&selector, timeout, None).await {
        Ok(_) => {
            println!("visible {selector_str}");
            Ok(())
        }
        Err(_) => Err(ActError::Timeout {
            selector: selector_str,
            timeout_ms: timeout.as_millis() as u64,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serialize env-touching tests. SMIX_RUNNER_PORT is a
    /// process-global, so the two `runner_port_from_env_*` tests must
    /// not race when cargo runs the test binary multi-threaded (default).
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn parse_selector_id() {
        let s = parse_selector("id:btn-take-photo").expect("parse id");
        assert!(matches!(s, Selector::Id { id, .. } if id == "btn-take-photo"));
    }

    #[test]
    fn parse_selector_text_plain() {
        let s = parse_selector("text:Welcome to smix").expect("parse text");
        match s {
            Selector::Text { text, .. } => {
                assert!(matches!(text, Pattern::Text(t) if t == "Welcome to smix"));
            }
            _ => panic!("expected Selector::Text"),
        }
    }

    #[test]
    fn parse_selector_label() {
        let s = parse_selector("label:Settings").expect("parse label");
        assert!(matches!(s, Selector::Label { label, .. } if label == "Settings"));
    }

    #[test]
    fn parse_selector_role_button() {
        let s = parse_selector("role:button").expect("parse role");
        assert!(matches!(
            s,
            Selector::Role {
                role: smix_screen::Role::Button,
                ..
            }
        ));
    }

    #[test]
    fn parse_selector_unknown_kind_returns_none() {
        assert!(parse_selector("xpath://*[1]").is_none());
        assert!(parse_selector("nope").is_none()); // no colon
    }

    #[test]
    fn runner_port_from_env_default_when_unset() {
        let _g = ENV_LOCK.lock().unwrap();
        // SAFETY: ENV_LOCK serializes env churn across the 2 tests in this
        // module that touch SMIX_RUNNER_PORT.
        unsafe { std::env::remove_var("SMIX_RUNNER_PORT") };
        assert_eq!(runner_port_from_env(), DEFAULT_RUNNER_PORT);
    }

    #[test]
    fn runner_port_from_env_reads_override() {
        let _g = ENV_LOCK.lock().unwrap();
        // SAFETY: as above.
        unsafe { std::env::set_var("SMIX_RUNNER_PORT", "22099") };
        assert_eq!(runner_port_from_env(), 22099);
        unsafe { std::env::remove_var("SMIX_RUNNER_PORT") };
    }

    #[test]
    fn parse_key_name_canonical_and_aliases() {
        assert_eq!(parse_key_name("return"), Some(KeyName::Return));
        assert_eq!(parse_key_name("enter"), Some(KeyName::Return));
        assert_eq!(parse_key_name("delete"), Some(KeyName::Delete));
        assert_eq!(parse_key_name("backspace"), Some(KeyName::Delete));
        assert_eq!(parse_key_name("arrowUp"), Some(KeyName::ArrowUp));
        assert_eq!(parse_key_name("up"), Some(KeyName::ArrowUp));
        assert_eq!(parse_key_name("escape"), Some(KeyName::Escape));
        assert_eq!(parse_key_name("esc"), Some(KeyName::Escape));
        assert_eq!(parse_key_name("volumeUp"), Some(KeyName::VolumeUp));
        assert_eq!(parse_key_name("volume-up"), Some(KeyName::VolumeUp));
        assert_eq!(parse_key_name("nope"), None);
    }

    #[test]
    fn parse_direction_four_compass_dirs() {
        assert_eq!(parse_direction("up"), Some(SwipeDirection::Up));
        assert_eq!(parse_direction("down"), Some(SwipeDirection::Down));
        assert_eq!(parse_direction("left"), Some(SwipeDirection::Left));
        assert_eq!(parse_direction("right"), Some(SwipeDirection::Right));
        assert_eq!(parse_direction("nope"), None);
        assert_eq!(parse_direction("UP"), None); // case-sensitive
    }
}
