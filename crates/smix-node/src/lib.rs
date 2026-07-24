//! napi-rs binding of the smix wire client for the TypeScript/Node SDK.
//!
//! The peer of `smix-ffi`'s UniFFI surface: `smix-ffi/src/driving.rs` wraps
//! one `HttpRunnerClient` and exposes it to Swift/Kotlin through UniFFI; this
//! crate wraps the same client and exposes it to Node through napi. Two
//! binding models, one wire client — kept in separate crates so a change to
//! either binding never drags the other's consumers.

use std::sync::Arc;

use napi_derive::napi;
use smix_input::{KeyName, SwipeDirection};
use smix_runner_client::{
    HttpRunnerClient, RunnerTransportError, SessionAppLifecycleRequest, SessionOpenRequest,
    SessionRelaunchAppRequest,
};
use smix_screen::A11yNode;
use smix_selector::Selector;

/// A wire transport failure is a cross-language contract report, not a
/// defensive catch — the JS caller needs the reason, so it surfaces as a
/// rejected Promise rather than a swallowed error.
fn transport_err(e: RunnerTransportError) -> napi::Error {
    napi::Error::from_reason(e.to_string())
}

/// A serialization failure of a wire type is a programmer error (the type
/// derives Serialize); it still crosses the boundary as a reason string
/// rather than panicking the Node process.
fn json_err(e: serde_json::Error) -> napi::Error {
    napi::Error::from_reason(e.to_string())
}

/// Parse a camelCase wire-enum name at the boundary, reusing the enum's own
/// serde mapping so the name table lives in exactly one place.
fn parse_wire_enum<T: serde::de::DeserializeOwned>(name: &str, what: &str) -> napi::Result<T> {
    serde_json::from_value(serde_json::Value::String(name.to_string()))
        .map_err(|_| napi::Error::from_reason(format!("unknown {what}: {name:?}")))
}

#[napi]
pub struct SmixNodeDriver {
    client: Arc<HttpRunnerClient>,
}

#[napi]
impl SmixNodeDriver {
    /// Build a driver aimed at a runner on `port`. Construction connects to
    /// nothing — it only builds the reqwest-backed client — so a never-served
    /// port is fine until the first call. `port` is `u32` because napi has no
    /// native `u16`; the client's port is `u16`, so the boundary narrows here.
    #[napi(constructor)]
    pub fn new(port: u32) -> Self {
        Self {
            client: Arc::new(HttpRunnerClient::new(port as u16)),
        }
    }

    /// Tap the normalized point `(nx, ny)` (each 0..1) and return the hit
    /// chain as JSON — the named elements containing the point, innermost
    /// first, the same `TapAtCoordResult` the wire returns. JSON string
    /// rather than a marshaled struct keeps C1's boundary minimal; the Node
    /// side reads `.chain`. The Arc is cloned before the await so the future
    /// owns its client and never borrows `&self` across napi's runtime.
    #[napi]
    pub async fn tap_at_coord(&self, nx: f64, ny: f64) -> napi::Result<String> {
        let client = Arc::clone(&self.client);
        let result = client.tap_at_norm_coord(nx, ny).await.map_err(transport_err)?;
        serde_json::to_string(&result).map_err(json_err)
    }

    /// Tap the element with accessibility identifier `id`. Returns whether a
    /// matching element was found and tapped — selectors resolve to an id in
    /// the SDK layer, so no selector crosses the boundary (parity with the
    /// UniFFI surface).
    #[napi]
    pub async fn tap_by_id(&self, id: String) -> napi::Result<bool> {
        let client = Arc::clone(&self.client);
        client.tap_by_id(&id).await.map_err(transport_err)
    }

    /// Type `text` into the focused element.
    #[napi]
    pub async fn input_text(&self, text: String) -> napi::Result<()> {
        let client = Arc::clone(&self.client);
        client.input_text(&text).await.map_err(transport_err)
    }

    /// Press a hardware/keyboard key by its camelCase wire name (e.g.
    /// `return`, `arrowUp`). The keyboard diagnostic payload is dropped —
    /// fire-and-return, matching the UniFFI surface.
    #[napi]
    pub async fn press_key(&self, key: String) -> napi::Result<()> {
        let key: KeyName = parse_wire_enum(&key, "key")?;
        let client = Arc::clone(&self.client);
        client.press_key(key).await.map_err(transport_err)?;
        Ok(())
    }

    /// Swipe once in a camelCase direction (`up`/`down`/`left`/`right`).
    #[napi]
    pub async fn swipe(&self, direction: String) -> napi::Result<()> {
        let direction: SwipeDirection = parse_wire_enum(&direction, "direction")?;
        let client = Arc::clone(&self.client);
        client.swipe_once(direction).await.map_err(transport_err)?;
        Ok(())
    }

    /// Snapshot the accessibility tree as JSON — the full scope (`None`), the
    /// same convention as the UniFFI surface. The Node side parses it.
    #[napi]
    pub async fn snapshot_tree(&self) -> napi::Result<String> {
        let client = Arc::clone(&self.client);
        let tree = client.get_tree(None).await.map_err(transport_err)?;
        serde_json::to_string(&tree).map_err(json_err)
    }

    /// The system popups currently on screen, as JSON.
    #[napi]
    pub async fn system_popups(&self) -> napi::Result<String> {
        let client = Arc::clone(&self.client);
        let popups = client.system_popups(None).await.map_err(transport_err)?;
        serde_json::to_string(&popups).map_err(json_err)
    }

    /// Open a session bound to `bundle_id` and return a handle for the
    /// lifecycle verbs. The session shares this driver's client (connection
    /// pool). `activate: false` mirrors the UniFFI surface — the caller
    /// foregrounds explicitly.
    #[napi]
    pub async fn open_session(&self, bundle_id: String) -> napi::Result<SmixNodeSession> {
        let client = Arc::clone(&self.client);
        let req = SessionOpenRequest {
            bundle_id,
            activate: false,
        };
        let resp = client.open_session(&req).await.map_err(transport_err)?;
        Ok(SmixNodeSession {
            client,
            session_id: resp.session_id,
        })
    }
}

/// A handle to an open runner session. Only the lifecycle verbs need the
/// session id, so they live here; the stateless act/sense verbs stay on the
/// driver.
#[napi]
pub struct SmixNodeSession {
    client: Arc<HttpRunnerClient>,
    session_id: String,
}

#[napi]
impl SmixNodeSession {
    /// Launch (or relaunch fresh) the session's app.
    #[napi]
    pub async fn launch_app(&self) -> napi::Result<()> {
        let client = Arc::clone(&self.client);
        let req = SessionAppLifecycleRequest {
            session_id: self.session_id.clone(),
            ..Default::default()
        };
        client.launch_session_app(&req).await.map_err(transport_err)?;
        Ok(())
    }

    /// Terminate the session's app.
    #[napi]
    pub async fn terminate_app(&self) -> napi::Result<()> {
        let client = Arc::clone(&self.client);
        let req = SessionAppLifecycleRequest {
            session_id: self.session_id.clone(),
            ..Default::default()
        };
        client
            .terminate_session_app(&req)
            .await
            .map_err(transport_err)?;
        Ok(())
    }

    /// Terminate and relaunch the session's app in one cycle.
    #[napi]
    pub async fn relaunch_app(&self) -> napi::Result<()> {
        let client = Arc::clone(&self.client);
        let req = SessionRelaunchAppRequest {
            session_id: self.session_id.clone(),
        };
        client
            .relaunch_session_app(&req)
            .await
            .map_err(transport_err)?;
        Ok(())
    }
}

// ---- host-side selector resolution (pure stone, no wire) --------------
//
// The napi peer of smix-ffi's UniFFI resolver exports: the TS SDK resolves a
// selector against a tree HERE (in-process, over the napi boundary), exactly
// as the Swift/Kotlin SDKs resolve host-side through UniFFI — no `/select/
// resolve` round-trip to a runner that never served it. Mirrors
// `smix-ffi/src/lib.rs`; the single source of resolution logic is the
// `smix-selector-resolver` stone.

fn parse_tree(tree_json: &str) -> napi::Result<A11yNode> {
    serde_json::from_str(tree_json)
        .map_err(|e| napi::Error::from_reason(format!("invalid tree JSON: {e}")))
}

fn parse_selector(selector_json: &str) -> napi::Result<Selector> {
    serde_json::from_str(selector_json)
        .map_err(|e| napi::Error::from_reason(format!("invalid selector JSON: {e}")))
}

fn modifiers_has_index(m: &smix_selector::Modifiers) -> bool {
    m.nth.is_some() || m.first.is_some() || m.last.is_some()
}

/// True when the selector carries an index modifier (first/last/nth), so a
/// single result (index pick) is wanted rather than every match. Mirrors
/// `smix-ffi::has_index_modifier`.
fn has_index_modifier(selector: &Selector) -> bool {
    use smix_selector::Selector as S;
    match selector {
        S::Text { modifiers, .. }
        | S::Id { modifiers, .. }
        | S::Label { modifiers, .. }
        | S::Role { modifiers, .. }
        | S::LocalizedText { modifiers, .. } => modifiers_has_index(modifiers),
        S::Anchor { index, .. } => {
            index.nth.is_some() || index.first.is_some() || index.last.is_some()
        }
        _ => false,
    }
}

/// Resolve a selector against a tree and return the matched identifiers.
/// Index modifiers pick a single result; otherwise every match's id.
#[napi]
pub fn resolve_selector(tree_json: String, selector_json: String) -> napi::Result<Vec<String>> {
    let tree = parse_tree(&tree_json)?;
    let selector = parse_selector(&selector_json)?;
    if has_index_modifier(&selector) {
        Ok(smix_selector_resolver::resolve_selector(&tree, &selector)
            .map(|node| vec![node.identifier.clone().unwrap_or_default()])
            .unwrap_or_default())
    } else {
        Ok(smix_selector_resolver::resolve_selector_all(&tree, &selector)
            .iter()
            .map(|node| node.identifier.clone().unwrap_or_default())
            .collect())
    }
}

/// Count selector matches against a tree (ignores index modifiers, matching
/// Playwright "match all" semantics).
#[napi]
pub fn resolve_selector_count(tree_json: String, selector_json: String) -> napi::Result<u32> {
    let tree = parse_tree(&tree_json)?;
    let selector = parse_selector(&selector_json)?;
    let matches = smix_selector_resolver::resolve_selector_all(&tree, &selector);
    Ok(u32::try_from(matches.len()).unwrap_or(u32::MAX))
}

/// Resolve a selector and return each match's accessibility label (empty
/// string where a match has none), aligned 1:1 with the match set.
#[napi]
pub fn resolve_selector_labels(
    tree_json: String,
    selector_json: String,
) -> napi::Result<Vec<String>> {
    let tree = parse_tree(&tree_json)?;
    let selector = parse_selector(&selector_json)?;
    Ok(smix_selector_resolver::resolve_selector_all(&tree, &selector)
        .iter()
        .map(|node| node.label.clone().unwrap_or_default())
        .collect())
}
