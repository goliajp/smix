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

use smix_driver::{AndroidDriver, Driver, Platform, SimctlDriver};
use smix_input::{KeyName, SwipeDirection};
use smix_runner_client::HttpRunnerClient;
use smix_selector::{Modifiers, Pattern, Selector};
use std::time::Duration;

/// The port the runner binds when nobody says otherwise.
///
/// The bottom rung of the ladder, and the only place it is spelled:
/// `run_port` reads it from here so the two paths cannot drift.
pub const DEFAULT_RUNNER_PORT: u16 = 22087;

/// Read SMIX_RUNNER_PORT env, or nothing.
///
/// Deliberately not `-> u16`. It used to be, substituting 22087 for an
/// unset variable, and that answer arrived before the registry was ever
/// asked: the documented ladder is flag → env → registry → default, and
/// a function that applies the default in the middle of it removes the
/// third rung. In a workspace with a sim registered on 22088 the
/// single-shot verbs dialled 22087 while `smix run` dialled 22088. The
/// default now lives in one place, at the bottom of the ladder.
pub fn runner_port_from_env_opt() -> Option<u16> {
    std::env::var("SMIX_RUNNER_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
}

// What `<kind>:<value>` accepts, form by form. Read by
// `scripts/dev/selector-surface-scan.py`; a `none` carries its reason.
// selector-surface: Text — `text:Save`
// selector-surface: Id — `id:btn-submit`
// selector-surface: Label — `label:Close`
// selector-surface: Role — `role:button`, and `Button` / `text_field` / `heading` / `tab` too: role_from_name, the same vocabulary yaml and MCP read
// selector-surface: Focused — none, the CLI has no verb whose target is whatever holds focus
// selector-surface: Anchor — none, modifiers have no shorthand at all here — a spatial chain needs more than one token
// selector-surface: LocalizedText — none, the shorthand carries one value and this form needs a locale list beside it
// selector-surface: OcrText — `ocrText:Total`, dispatched past the tree by every cmd that acts on it
// selector-surface-field: OcrText.locales — `--ocr-locale zh-Hans`, repeatable, best first; a value cannot ride in the token because `text:` may legally contain any character
// selector-surface-field: Role.name — none, `role:` carries one value and narrowing needs a second; nobody has wired the flag
// selector-surface: AnchorRelative — none, an anchor plus two offsets does not fit one token
// selector-surface: Point — `point:50%,25%`, dispatched by cmd_tap and refused by the rest
// selector-surface: Fallback — none, a chain in one token needs a separator, and `text:` may legally contain any character

/// Parse `<kind>:<value>` selector shorthand.
///
/// `Err` carries the sentence to print. It used to be `Option`, and the
/// caller turned `None` into "expected one of id / text / label / role"
/// whatever had gone wrong — so `point:267,100` was answered with a list
/// of kinds rather than the one thing the writer needed to hear, which is
/// that the unit is a fraction of the viewport. yaml has said that for a
/// while. §9 #1 ③: name what is wrong, never degrade into a shrug.
pub fn parse_selector(s: &str) -> Result<Selector, String> {
    let Some((kind, value)) = s.split_once(':') else {
        return Err(format!(
            "`{s}` has no kind — write `<kind>:<value>`, one of \
             id / text / label / role / ocrText / point"
        ));
    };
    let modifiers = Modifiers::default();
    match kind {
        "id" => Ok(Selector::Id {
            id: value.to_string(),
            modifiers,
        }),
        "text" => Ok(Selector::Text {
            text: Pattern::text(value),
            modifiers,
        }),
        "label" => Ok(Selector::Label {
            label: value.to_string(),
            modifiers,
        }),
        // `role_from_name`, not `role_from_raw_type`. The wire form reads
        // the strings the runner sends; a person types the word. Every
        // word the wire form took, this one takes too and maps the same
        // way — the reverse difference is empty — so this is widening, and
        // what it adds is what a reader writes first: `Button` with its
        // capital, `text_field` with its underscore, `heading` and `tab`,
        // which have no wire spelling at all.
        "role" => {
            let role = smix_selector::role_from_name(value).ok_or_else(|| {
                format!(
                    "unknown role `{value}`; accepted: {}",
                    smix_selector::ROLE_NAMES
                )
            })?;
            Ok(Selector::Role {
                role,
                name: None,
                modifiers,
            })
        }
        // The vision path. It never matches in the tree — the resolver
        // returns false for OcrText by design, because OCR is a live
        // look at the screen and not a predicate over a dump — so every
        // command that takes it has to dispatch past the tree. Forgetting
        // that is not an error: it is a silent miss on text that is
        // plainly there.
        "ocrText" => Ok(Selector::OcrText {
            ocr_text: value.to_string(),
            locales: Vec::new(),
            modifiers,
        }),
        // A place rather than a thing, and the same reading as yaml and
        // MCP because it is the same function. Only `tap` takes one —
        // there is nothing at a coordinate to find, fill or wait for.
        "point" => {
            let (nx, ny) = smix_selector::point_from_str(value)?;
            Ok(Selector::Point { nx, ny })
        }
        _ => Err(format!(
            "unknown selector kind `{kind}` — one of id / text / label / role / ocrText / point"
        )),
    }
}

/// Where an OCR needle is on screen, or nothing.
///
/// `OcrText` never matches in the tree — the resolver returns false for
/// it by design, since OCR is a live look and not a predicate over a
/// dump. So every command that accepts one dispatches here instead of
/// resolving, and forgetting to is a silent miss on text that is plainly
/// on screen rather than an error anyone would see.
async fn ocr_frame(
    d: &dyn Driver,
    needle: &str,
    locales: &[String],
) -> Result<Option<smix_driver::OcrFrame>, ActError> {
    d.find_text_by_ocr(needle, locales, smix_driver::OCR_RECOGNITION_LEVEL)
        .await
        .map_err(|e| ActError::Transport(e.to_prompt()))
}

/// The needle, when the selector is the vision path.
fn ocr_needle(sel: &Selector) -> Option<&str> {
    match sel {
        Selector::OcrText { ocr_text, .. } => Some(ocr_text),
        _ => None,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ActError {
    #[error("invalid selector `{0}`: {1}")]
    BadSelector(String, String),
    #[error("runner transport: {0}")]
    Transport(String),
    #[error("wait_for timeout after {timeout_ms}ms: {selector}")]
    Timeout { selector: String, timeout_ms: u64 },
}

/// Dial the driver a CLI verb should drive through, by platform. The act
/// verbs used to build a `SimctlDriver` unconditionally, so `fill` and
/// `find text:` were 501 on Android while the flow path — which already
/// picks `AndroidDriver` — worked on the same device. This is the driver
/// half of C3: the same capability reaches the same driver from every
/// entrance. iOS-only sense (`describe`) stays on `SimctlDriver` because
/// it is an inherent method, not on the `Driver` trait.
fn driver_for(platform: Platform, port: u16) -> Box<dyn Driver> {
    let client = HttpRunnerClient::new(port);
    match platform {
        Platform::Android => Box::new(AndroidDriver::new(client)),
        Platform::Ios => Box::new(SimctlDriver::new(client)),
    }
}

#[cfg(test)]
mod driver_for {
    use super::*;

    // C3: the act verbs must reach the driver the device's platform
    // names, not always the simctl one. Building it and asking its
    // platform back pins the dispatch without a device.
    #[test]
    fn dispatches_android_to_the_android_driver() {
        assert_eq!(
            driver_for(Platform::Android, 22088).platform(),
            Platform::Android
        );
    }

    #[test]
    fn dispatches_ios_to_the_ios_driver() {
        assert_eq!(driver_for(Platform::Ios, 22087).platform(), Platform::Ios);
    }
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
/// `smix tap <selector> --then-screenshot <out>`.
///
/// The tap and the frame in one command, in that order. What this saves
/// is not wire time — a tap is about 336 ms and a frame from the runner
/// about 88 ms, and a UI that hides itself after three seconds outlives
/// both. It saves the gap between two commands, which is where the
/// seconds actually went for the consumer who asked for it.
pub async fn cmd_tap_then_screenshot(
    selector_str: String,
    port: u16,
    platform: Platform,
    out: &std::path::Path,
) -> Result<(), ActError> {
    let selector = parse_selector(&selector_str)
        .map_err(|why| ActError::BadSelector(selector_str.clone(), why))?;
    if ocr_needle(&selector).is_some() {
        return Err(ActError::BadSelector(
            selector_str,
            "--then-screenshot needs a selector the tree can resolve. An \
             ocrText hit is a text frame rather than a resolved element, \
             so there would be nothing to say about where the touch \
             landed — which is why this command takes no --ocr-locale"
                .to_string(),
        ));
    }
    let d = driver_for(platform, port);
    let raw = HttpRunnerClient::new(port);
    let (outcome, captured) = smix_sdk::tap_then_capture_with(d.as_ref(), Some(&raw), &selector)
        .await
        .map_err(|e| ActError::Transport(e.to_prompt()))?;
    // Only now: a tap that failed returned above, so nothing on disk can
    // be mistaken for a picture of something that happened.
    std::fs::write(out, &captured.png)
        .map_err(|e| ActError::Transport(format!("write {}: {e}", out.display())))?;
    println!(
        "tapped: {selector_str} — frame via {} {} ms later, {} bytes to {}",
        captured.via,
        captured.gap_ms,
        captured.png.len(),
        out.display()
    );
    if let smix_driver::ActVerdict::Unconfirmable(why) = &outcome.verdict {
        println!("  not verified: {why}");
    }
    Ok(())
}

pub async fn cmd_tap(
    selector_str: String,
    port: u16,
    platform: Platform,
    ocr_locales: Vec<String>,
) -> Result<(), ActError> {
    let selector = parse_selector(&selector_str)
        .map_err(|why| ActError::BadSelector(selector_str.clone(), why))?;
    let d = driver_for(platform, port);
    // A place, not a thing: the resolver has nothing to match, and its own
    // comment says so — reaching it means a caller forgot to dispatch. The
    // touch goes straight to the coordinate, the same as `tapOn: { point }`.
    if let Some(needle) = ocr_needle(&selector) {
        return match ocr_frame(d.as_ref(), needle, &ocr_locales).await? {
            Some(f) => {
                d.tap_at_norm_coord(f.mid_x(), f.mid_y())
                    .await
                    .map_err(|e| ActError::Transport(e.to_prompt()))?;
                println!(
                    "tapped: ocrText={needle} at ({:.3}, {:.3}) — not verified: an OCR hit is a text frame, not a resolved element",
                    f.mid_x(),
                    f.mid_y()
                );
                Ok(())
            }
            None => Err(ActError::Timeout {
                selector: format!("ocrText:{needle}"),
                timeout_ms: 0,
            }),
        };
    }
    if let Selector::Point { nx, ny } = selector {
        d.tap_at_norm_coord(nx, ny)
            .await
            .map_err(|e| ActError::Transport(e.to_prompt()))?;
        println!(
            "tapped: point=({nx:.3}, {ny:.3}) — not verified: this path sends a touch without resolving a target"
        );
        return Ok(());
    }
    let outcome = d
        .tap(&selector, None)
        .await
        .map_err(|e| ActError::Transport(format!("{e}")))?;
    // Say what the touch landed on, not just that one was sent.
    //
    // A consumer hit "the step is green and the app did nothing" three
    // times in one effort, and all three times the touch HAD arrived —
    // the fault was downstream, in a counter, a no-op navigate, and an
    // inter-tap window. Each time the green step was read as "smix did
    // not deliver it" and sent them upstream first, ~13 rounds in
    // total. A pass that shows its evidence points the other way.
    println!("tapped {selector_str}");
    if !outcome.observed.is_empty() {
        let inside: Vec<String> = outcome
            .observed
            .iter()
            .map(|e| {
                if !e.identifier.is_empty() {
                    e.identifier.clone()
                } else if !e.label.is_empty() {
                    format!("{:?}", e.label)
                } else {
                    "<unnamed>".to_string()
                }
            })
            .collect();
        // "aimed", not "landed". What the runner computed is geometry —
        // every named element whose frame contains the point, as the
        // snapshot describes it. A landscape screen returns exactly this
        // line for a touch that never moves a pixel: the point is worked
        // out in the app's space and delivered stamped with the device's.
        println!("  aimed inside: {}", inside.join(" < "));
    }
    if let smix_driver::ActVerdict::Unconfirmable(why) = &outcome.verdict {
        println!("  not verified: {why}");
    }
    Ok(())
}

/// `smix find <selector>` — boolean existence probe. Same routing path as
/// `smix tap`: text → swift /find shortcut, anything else → /tree resolve.
pub async fn cmd_find(
    selector_str: String,
    port: u16,
    platform: Platform,
    ocr_locales: Vec<String>,
) -> Result<(), ActError> {
    let selector = parse_selector(&selector_str)
        .map_err(|why| ActError::BadSelector(selector_str.clone(), why))?;
    if matches!(selector, Selector::Point { .. }) {
        return Err(ActError::BadSelector(
            selector_str.clone(),
            "a point names a place, not an element, so there is nothing here to \
             find. Only `smix tap` takes one"
                .into(),
        ));
    }
    let d = driver_for(platform, port);
    if let Some(needle) = ocr_needle(&selector) {
        let exists = ocr_frame(d.as_ref(), needle, &ocr_locales).await?.is_some();
        println!("exists={exists}");
        return Ok(());
    }
    let exists = d
        .find(&selector, None)
        .await
        .map_err(|e| ActError::Transport(format!("{e}")))?;
    println!("exists={exists}");
    Ok(())
}

/// `smix fill <selector> --text <text>` — type `text` into the matched field.
/// Mirrors maestro `inputText:`.
pub async fn cmd_fill(
    selector_str: String,
    text: String,
    port: u16,
    platform: Platform,
) -> Result<(), ActError> {
    let selector = parse_selector(&selector_str)
        .map_err(|why| ActError::BadSelector(selector_str.clone(), why))?;
    if ocr_needle(&selector).is_some() {
        return Err(ActError::BadSelector(
            selector_str.clone(),
            "an OCR hit is a text frame on the screen, not a field with a value — find it with `smix find ocrText:…` or tap it, then act on \
             what the tap put on screen"
                .into(),
        ));
    }
    if matches!(selector, Selector::Point { .. }) {
        return Err(ActError::BadSelector(
            selector_str.clone(),
            "a point names a place, not an element, so there is nothing here to \
             fill. Only `smix tap` takes one"
                .into(),
        ));
    }
    let d = driver_for(platform, port);
    // Same rule as the SDK: a named field is replaced, the focused one
    // is typed into. See `App::fill`.
    let names_a_field = !matches!(selector, smix_selector::Selector::Focused { .. });
    d.fill(&selector, &text, None, names_a_field)
        .await
        // Same refusal, same sentence, one definition of it — see
        // `smix_sdk::focused_fill_refusal`.
        .map_err(|e| {
            if names_a_field {
                e
            } else {
                smix_sdk::focused_fill_refusal(e)
            }
        })
        .map_err(|e| ActError::Transport(format!("{e}")))?;
    // The value never comes back out. smix's output is a transcript that
    // persists and is read by an AI, so a password typed through here
    // would live in the record forever — and it did: a staging account's
    // password reached a session log on 2026-08-09 because this line
    // echoed what the caller had deliberately kept in a shell variable.
    //
    // Not a `--secret` flag and not secure-field detection: a default
    // that protects you only when you remember to ask for it is not a
    // default, and detection fails open on the fields it cannot read.
    // The length is confirmation enough — the caller already knows the
    // value, and what they need to know is that it arrived whole.
    println!("filled {selector_str} ({} chars)", text.chars().count());
    Ok(())
}

/// `smix press-key <key-name>` — issue a hardware / IME key press. Key
/// shorthand: `return` (alias `enter`), `delete` (alias `backspace`),
/// `tab`, `space`, `escape` / `esc`, `arrowUp` / `up`, `arrowDown` /
/// `down`, `arrowLeft` / `left`, `arrowRight` / `right`, `home`, `lock`,
/// `volumeUp` / `volume-up`, `volumeDown` / `volume-down`.
pub async fn cmd_press_key(key_str: String, port: u16, platform: Platform) -> Result<(), ActError> {
    let key = parse_key_name(&key_str).ok_or_else(|| {
        ActError::BadSelector(format!("key:{key_str}"), "unknown key name".into())
    })?;
    let d = driver_for(platform, port);
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
    platform: Platform,
) -> Result<(), ActError> {
    let selector = parse_selector(&selector_str)
        .map_err(|why| ActError::BadSelector(selector_str.clone(), why))?;
    if ocr_needle(&selector).is_some() {
        return Err(ActError::BadSelector(
            selector_str.clone(),
            "scrolling needs an element the tree can follow; an OCR frame is where text was one look ago — find it with `smix find ocrText:…` or tap it, then act on \
             what the tap put on screen"
                .into(),
        ));
    }
    if matches!(selector, Selector::Point { .. }) {
        return Err(ActError::BadSelector(
            selector_str.clone(),
            "a point names a place, not an element, so there is nothing here to \
             scroll to. Only `smix tap` takes one"
                .into(),
        ));
    }
    let direction = parse_direction(&direction_str).ok_or_else(|| {
        ActError::BadSelector(
            format!("direction:{direction_str}"),
            "expected up / down / left / right".into(),
        )
    })?;
    let d = driver_for(platform, port);
    d.scroll(&selector, direction)
        .await
        .map_err(|e| ActError::Transport(format!("{e}")))?;
    println!("scrolled {direction_str} to {selector_str}");
    Ok(())
}

/// `smix swipe <direction>` — one swipe through the content.
///
/// `direction` names what the caller wants to SEE, which is the same
/// contract `smix_swipe` states; both reach `swipe_once`, so the word
/// cannot mean one thing here and another there.
/// Swipe between two normalised points.
///
/// The authorised coordinate escape hatch for swipe (§9 #3), on the
/// same grounds as tap's: a screen with nothing nameable to swipe
/// between leaves a flow author with no other move. Two points rather
/// than one, because a swipe is a path — tap's single-point shape does
/// not describe it.
pub async fn cmd_swipe_between(
    from: &str,
    to: &str,
    port: u16,
    platform: Platform,
) -> Result<(), ActError> {
    // The same parser the `point:` selector uses, so `50%,80%` and
    // `0.5,0.8` mean the same thing here as everywhere else and pixels
    // are refused with the same sentence.
    let from_pt = smix_selector::point_from_str(from)
        .map_err(|why| ActError::BadSelector(format!("--from {from}"), why))?;
    let to_pt = smix_selector::point_from_str(to)
        .map_err(|why| ActError::BadSelector(format!("--to {to}"), why))?;
    let d = driver_for(platform, port);
    d.swipe_at_norm_coord(from_pt, to_pt)
        .await
        .map_err(|e| ActError::Transport(format!("{e}")))?;
    println!(
        "swiped ({:.3}, {:.3}) → ({:.3}, {:.3})",
        from_pt.0, from_pt.1, to_pt.0, to_pt.1
    );
    Ok(())
}

pub async fn cmd_swipe(
    direction_str: String,
    port: u16,
    platform: Platform,
) -> Result<(), ActError> {
    let direction = parse_direction(&direction_str).ok_or_else(|| {
        ActError::BadSelector(
            format!("direction:{direction_str}"),
            "expected up / down / left / right".into(),
        )
    })?;
    let d = driver_for(platform, port);
    d.swipe_once(direction)
        .await
        .map_err(|e| ActError::Transport(format!("{e}")))?;
    println!("swiped {direction_str}");
    Ok(())
}

/// `smix tree [--json]` — print the runner's current accessibility tree.
/// `--json` emits the wire-format JSON (large — typically 100KB+ for a
/// typical app screen); default emits an indented text outline keyed by
/// id + label per node.
pub async fn cmd_tree(json: bool, port: u16, keyboard: bool) -> Result<(), ActError> {
    // The client rather than the driver, because the driver's `tree` hands
    // back the root alone and the source is the half a reader needs most
    // when the answer looks thin. A screen the accessibility reader has
    // gone blind on and a screen with nothing on it print identically
    // otherwise.
    let client = HttpRunnerClient::new(port);
    let mut perceived = client
        .get_tree(None)
        .await
        .map_err(|e| ActError::Transport(format!("{e}")))?;
    if !keyboard {
        collapse_keyboards(&mut perceived.root);
    }
    if json {
        let s = serde_json::to_string_pretty(&perceived)
            .map_err(|e| ActError::Transport(format!("serde: {e}")))?;
        println!("{s}");
    } else {
        if let Some(caveat) = perceived.caveat() {
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
fn collapse_keyboards(node: &mut smix_screen::A11yNode) -> usize {
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

fn outline_line(node: &smix_screen::A11yNode, depth: usize) -> String {
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

fn print_tree_outline(node: &smix_screen::A11yNode, depth: usize) {
    println!("{}", outline_line(node, depth));
    for child in &node.children {
        print_tree_outline(child, depth + 1);
    }
}

/// `smix describe [--json]` — print the runner's ScreenDescription: the
/// nameable visible elements, the bundle id the description was taken
/// from, and the capture timestamp. `--json` emits the wire JSON;
/// default emits a pretty-printed Debug summary.
///
/// It used to promise a title and a status bar. Neither exists anywhere
/// in the tree, and two of the three metadata fields were empty on top
/// of that — the help described a richer thing than the code produced.
pub async fn cmd_describe(json: bool, port: u16) -> Result<(), ActError> {
    let d = SimctlDriver::new(HttpRunnerClient::new(port));
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
    let d = SimctlDriver::new(HttpRunnerClient::new(port));
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

/// `smix system-popup-action` — press a named button on a SpringBoard
/// popup. Both ids come from `smix system-popups` output. Exit is an
/// error when the runner reports no such popup/button (404), so shell
/// callers can branch on it.
pub async fn cmd_system_popup_action(
    popup_id: &str,
    button_id: &str,
    port: u16,
) -> Result<(), ActError> {
    let d = SimctlDriver::new(HttpRunnerClient::new(port));
    let pressed = d
        .system_popup_action(popup_id, button_id)
        .await
        .map_err(|e| ActError::Transport(format!("{e}")))?;
    if !pressed {
        return Err(ActError::Transport(format!(
            "no popup {popup_id:?} with button {button_id:?} — list current \
             popups via `smix system-popups`"
        )));
    }
    println!("pressed: {button_id} on {popup_id}");
    Ok(())
}

/// `smix hide-keyboard` — dismiss the soft keyboard if visible.
pub async fn cmd_hide_keyboard(port: u16, platform: Platform) -> Result<(), ActError> {
    let d = driver_for(platform, port);
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
    platform: Platform,
    absent: bool,
    ocr_locales: Vec<String>,
) -> Result<(), ActError> {
    let selector = parse_selector(&selector_str)
        .map_err(|why| ActError::BadSelector(selector_str.clone(), why))?;
    if matches!(selector, Selector::Point { .. }) {
        return Err(ActError::BadSelector(
            selector_str.clone(),
            "a point names a place, not an element, so there is nothing here to \
             wait for. Only `smix tap` takes one"
                .into(),
        ));
    }
    let d = driver_for(platform, port);
    let timeout = Duration::from_secs(timeout_secs);
    // Waiting for text to appear is what the vision path is for, so this
    // polls rather than refusing. It cannot go through `wait_for`: that
    // resolves, and a resolve of an OCR needle is a no every time.
    if let Some(needle) = ocr_needle(&selector) {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let seen = ocr_frame(d.as_ref(), needle, &ocr_locales).await?.is_some();
            if seen != absent {
                println!("{} {selector_str}", if absent { "gone" } else { "visible" });
                return Ok(());
            }
            if std::time::Instant::now() >= deadline {
                return Err(ActError::Timeout {
                    selector: selector_str,
                    timeout_ms: timeout.as_millis() as u64,
                });
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
    // Both arms report a timeout the same way. The waits are opposites
    // but the failure is not: in either case the screen did not reach
    // the state the caller asked for within the budget.
    let outcome = if absent {
        d.wait_for_not_visible(&selector, timeout).await
    } else {
        d.wait_for(&selector, timeout, None).await.map(|_| ())
    };
    match outcome {
        Ok(()) => {
            println!("{} {selector_str}", if absent { "gone" } else { "visible" });
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
    /// No surface that takes a caller-supplied value may echo it.
    ///
    /// Source-level, and that is deliberate: the leak is a `println!`
    /// argument, so the thing to pin is that the argument is not there.
    /// A behavioural test would have to capture stdout of a command that
    /// needs a device, and would not run where this must hold — on every
    /// build, on every machine.
    ///
    /// Found in production: `smix fill` printed a staging account's
    /// password into a session transcript on 2026-08-09, defeating a
    /// caller who had deliberately kept it in a shell variable.
    #[test]
    fn no_surface_echoes_a_value_the_caller_supplied() {
        for (name, src) in [
            ("smix-cli/act.rs", include_str!("act.rs")),
            (
                "smix-mcp/main.rs",
                include_str!("../../smix-mcp/src/main.rs"),
            ),
        ] {
            for line in src.lines() {
                let l = line.trim();
                if !l.starts_with("//") && l.contains("filled") {
                    assert!(
                        !l.contains("{text}")
                            && !l.contains("params.text\"")
                            && !l.contains("`{text}`"),
                        "{name} echoes the filled value: {l}"
                    );
                }
            }
        }
    }

    use super::*;

    fn node_with(
        id: Option<&str>,
        label: Option<&str>,
        text: Option<&str>,
    ) -> smix_screen::A11yNode {
        smix_screen::A11yNode {
            raw_type: "other".into(),
            element_type_raw: 1,
            role: None,
            identifier: id.map(String::from),
            label: label.map(String::from),
            title: None,
            placeholder_value: None,
            value: None,
            text: text.map(String::from),
            bounds: smix_screen::Rect {
                x: 0.0,
                y: 0.0,
                w: 10.0,
                h: 10.0,
            },
            enabled: true,
            selected: false,
            has_focus: false,
            visible: true,
            children: vec![],
        }
    }

    // ⑤: the Android human tree showed only id + label, so `SUBMIT` (which
    // lives in `text` on Android, with label empty) was invisible while
    // present in --json. text has to print in its own position — not folded
    // into id/label — and an empty text must not leave a `text=` ghost.
    #[test]
    fn outline_line_shows_text_in_its_own_position() {
        let submit = node_with(Some("fixture_submit"), None, Some("SUBMIT"));
        let line = outline_line(&submit, 0);
        assert!(
            line.contains(r#"id="fixture_submit""#),
            "id in its place: {line}"
        );
        assert!(
            line.contains(r#"text="SUBMIT""#),
            "text in its place: {line}"
        );
        assert!(line.contains(r#"label="""#), "label still printed: {line}");
        assert!(
            !line.contains(r#"id="SUBMIT""#),
            "text must not bleed into id: {line}"
        );

        // A node with no text must not grow a `text=` field.
        let bare = node_with(Some("statusBarBackground"), None, None);
        assert!(
            !outline_line(&bare, 0).contains("text="),
            "empty text prints nothing: {}",
            outline_line(&bare, 0)
        );
    }

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

    /// One word, one meaning, on every surface a person types it into.
    ///
    /// The CLI read roles through `role_from_raw_type` — the wire's own
    /// `rawType` strings — while yaml and MCP read the same word through
    /// `role_from_name`, which is what a person writes. So `role:Button`
    /// worked in a flow and was refused here, and the refusal did not
    /// even list what was on offer. The reverse difference is empty: every
    /// word the wire form accepts, the written form accepts too and maps
    /// to the same Role. Widening loses nothing.
    #[test]
    fn the_cli_reads_the_same_role_vocabulary_as_yaml_and_mcp() {
        for (written, expected) in [
            ("role:Button", smix_screen::Role::Button),
            ("role:BUTTON", smix_screen::Role::Button),
            ("role:text_field", smix_screen::Role::TextField),
            ("role:heading", smix_screen::Role::StaticText),
            ("role:tab", smix_screen::Role::Tab),
        ] {
            let s = parse_selector(written)
                .unwrap_or_else(|e| panic!("{written} is accepted in yaml and MCP: {e}"));
            assert!(
                matches!(&s, Selector::Role { role, .. } if *role == expected),
                "{written} resolved to something else"
            );
        }
    }

    /// The words that already worked keep working — widening is not the
    /// same as replacing.
    #[test]
    fn the_wire_spellings_still_parse() {
        for w in ["role:button", "role:textField", "role:staticText"] {
            assert!(parse_selector(w).is_ok(), "{w}");
        }
    }

    /// A word in neither vocabulary is refused with the vocabulary shown.
    /// The old refusal said "unknown role `x`" and stopped there.
    #[test]
    fn an_unknown_role_shows_what_is_on_offer() {
        let e = parse_selector("role:widget").expect_err("not a role");
        assert!(e.contains("widget"), "{e}");
        assert!(
            e.contains("button"),
            "the accepted list has to be in it: {e}"
        );
    }

    /// Was `returns_none`. It returns the reason now — the same refusal,
    /// with the half a reader needs added to it.
    #[test]
    fn parse_selector_unknown_kind_says_why() {
        assert!(parse_selector("xpath://*[1]").is_err());
        assert!(parse_selector("nope").is_err()); // no colon
    }

    /// An unset variable is `None`, not the default.
    ///
    /// The distinction is the whole point: `None` lets the registry
    /// answer next, and 22087 would not.
    #[test]
    fn runner_port_from_env_is_none_when_unset() {
        let _g = ENV_LOCK.lock().unwrap();
        // SAFETY: ENV_LOCK serializes env churn across the 2 tests in this
        // module that touch SMIX_RUNNER_PORT.
        unsafe { std::env::remove_var("SMIX_RUNNER_PORT") };
        assert_eq!(runner_port_from_env_opt(), None);
    }

    #[test]
    fn runner_port_from_env_reads_override() {
        let _g = ENV_LOCK.lock().unwrap();
        // SAFETY: as above.
        unsafe { std::env::set_var("SMIX_RUNNER_PORT", "22099") };
        assert_eq!(runner_port_from_env_opt(), Some(22099));
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

#[cfg(test)]
mod point_selector_tests {
    use super::parse_selector;
    use smix_selector::Selector;

    /// The same reading as yaml and MCP, because it is the same function.
    #[test]
    fn a_point_parses_and_a_fraction_equals_a_percentage() {
        let a = parse_selector("point:50%,25%").expect("percentage");
        let b = parse_selector("point:0.5,0.25").expect("fraction");
        assert!(matches!(a, Selector::Point { nx, ny } if nx == 0.5 && ny == 0.25));
        assert_eq!(format!("{a:?}"), format!("{b:?}"));
    }

    /// The reason this returns `Result`. A list of accepted kinds is not an
    /// answer to "your number is in pixels".
    #[test]
    fn pixels_are_answered_with_the_unit_not_a_list_of_kinds() {
        let e = parse_selector("point:267,100").expect_err("267 is off screen");
        assert!(e.contains("fraction of the viewport"), "{e}");
        assert!(
            !e.contains("one of id / text"),
            "a kind list is not the answer: {e}"
        );
    }

    /// And the shapes that were always wrong still say what is wrong.
    #[test]
    fn an_unknown_kind_and_a_missing_colon_each_say_so() {
        assert!(
            parse_selector("xpath://div")
                .unwrap_err()
                .contains("unknown selector kind")
        );
        assert!(
            parse_selector("btn-submit")
                .unwrap_err()
                .contains("no kind")
        );
    }

    /// The four that were there before still parse — the point of adding a
    /// fifth is not to break the four.
    #[test]
    fn the_existing_kinds_still_parse() {
        // Lower case: the CLI reads roles through `role_from_raw_type`,
        // which is the wire's vocabulary, while yaml and MCP read the same
        // word through `role_from_name`. `role:Button` parses in two of the
        // three surfaces. That is the next gap on this axis and is not this
        // change's to close — it is written down in the decision log.
        for s in ["id:btn", "text:Save", "label:Close", "role:button"] {
            assert!(parse_selector(s).is_ok(), "{s}");
        }
    }
}
