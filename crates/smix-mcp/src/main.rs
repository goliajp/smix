//! smix-mcp — MCP server exposing smix tools to Claude Code via stdio.
//!
//! # Connection model
//!
//! Each MCP server process binds to *one* simulator's runner. UDID and
//! runner port come from environment:
//! - `SMIX_RUNNER_PORT` (default: 22087)
//! - `SMIX_UDID` (required for simctl-bound tools; sense+act tools without
//!   simctl don't need it)
//!
//! Claude Code or other MCP client launches this binary with stdio
//! transport; tools call into `smix_sdk::App` which fans out to driver +
//! runner-client + simctl.
//!
//! # Tools (MVP set)
//!
//! - `smix_describe` — return `ScreenDescription` of current screen
//! - `smix_tree` — return full A11yNode JSON
//! - `smix_find_text` — boolean existence of a text selector
//! - `smix_tap_text` — tap by text selector
//! - `smix_press_key` — press named key (Return / Tab / arrow keys)
//! - `smix_screenshot` — capture base64 PNG (UDID-bound)

use base64::Engine as _;
use rmcp::ErrorData as McpError;
use rmcp::ServiceExt;
use rmcp::handler::server::ServerHandler;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, Content, Implementation, ProtocolVersion, ServerCapabilities, ServerInfo,
};
use rmcp::transport::stdio;
use rmcp::{tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use smix_input::{KeyName, SwipeDirection};
use smix_mcp::{SelectorParams, ocr_text_of};
use smix_sdk::{App, KeyName as SdkKeyName};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

#[derive(Clone)]
struct SmixMcpService {
    /// Shared App (one per process; UDID + runner-port from env).
    app: Arc<Mutex<App>>,
    /// Tool router populated by #[tool_router] macro; read by the
    /// macro-generated `serve` plumbing, not by hand-written code.
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

/// Typing needs two strings — which field, and what to put in it — so the
/// selector is nested rather than flattened. Flattened, its `text` (find the
/// field by its visible text) and the value to type collide on one key, and
/// the only correct-looking call, `{id, text}`, gets rejected as naming two
/// selectors.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct FillParams {
    /// Which field to type into. Give exactly one of id / text / label /
    /// role / ocrText — prefer id.
    target: SelectorParams,
    /// The text to type.
    text: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct SwipeParams {
    /// Which way to travel through the content: up, down, left, or right.
    /// This names what you want to SEE — "down" reveals what is below,
    /// whichever way the finger has to move to get there.
    direction: String,
}

/// Nested rather than flattened, to match `smix_fill` — the two tools that
/// take a selector plus something else should read the same way.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ScrollParams {
    /// Which element to bring into view. Give exactly one of id / text /
    /// label / role / ocrText — prefer id.
    target: SelectorParams,
    /// Which way to travel through the content: up, down, left, or right.
    /// Names what you want to SEE, not the finger's direction. Defaults to
    /// "down".
    #[serde(default)]
    direction: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct BundleParams {
    /// The app's bundle id, e.g. com.example.app.
    bundle_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct PressKeyParams {
    /// Key name. One of: return / delete / tab / space / escape /
    /// arrowUp / arrowDown / arrowLeft / arrowRight.
    key: String,
}

/// The SDK's own no-UDID failure tells the caller to use `.with_udid(...)`
/// — a Rust API an MCP caller cannot reach. Say the thing they can do.
fn missing_udid_error() -> McpError {
    McpError::invalid_params(
        "SMIX_UDID is not set — set the SMIX_UDID env var in this MCP server's \
         config to the target simulator's UDID (find it with `xcrun simctl list \
         devices`), then restart the server",
        None,
    )
}

#[tool_router]
impl SmixMcpService {
    fn new(app: App) -> Self {
        Self {
            app: Arc::new(Mutex::new(app)),
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Get a structured description of the current screen — visible elements + bounds. Needs the session smix_launch_app opens (SMIX_UDID env var set)."
    )]
    async fn smix_describe(&self) -> Result<CallToolResult, McpError> {
        let app = self.app.lock().await;
        let desc = app
            .describe()
            .await
            .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
        let json = serde_json::to_string_pretty(&desc).unwrap_or_default();
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        description = "Get the raw A11yNode tree of the current screen. Needs the session smix_launch_app opens (SMIX_UDID env var set)."
    )]
    async fn smix_tree(&self) -> Result<CallToolResult, McpError> {
        let app = self.app.lock().await;
        let tree = app
            .tree()
            .await
            .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
        let json = serde_json::to_string_pretty(&tree).unwrap_or_default();
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        description = "Check whether an element is on screen, as a plain true/false. Use this to look before you act; use smix_assert_visible when absence should be a failure. An ocrText selector runs an Apple Vision OCR pass. Needs the session smix_launch_app opens (SMIX_UDID env var set)."
    )]
    async fn smix_find(
        &self,
        Parameters(params): Parameters<SelectorParams>,
    ) -> Result<CallToolResult, McpError> {
        let sel = params.to_selector()?;
        let app = self.app.lock().await;
        // The tree resolver never matches OcrText (live-vision op, not a
        // tree predicate) — routed through `app.find` an ocrText selector
        // is always false. Dispatch it to the OCR path instead, as the
        // maestro adapter does.
        let exists = match ocr_text_of(&sel) {
            Some(needle) => app
                .find_by_text_ocr(needle, &[])
                .await
                .map_err(|e| McpError::internal_error(e.to_prompt(), None))?
                .is_some(),
            None => app
                .find(&sel)
                .await
                .map_err(|e| McpError::internal_error(e.to_prompt(), None))?,
        };
        Ok(CallToolResult::success(vec![Content::text(
            if exists { "true" } else { "false" }.to_string(),
        )]))
    }

    #[tool(
        description = "Tap an element. Name it with exactly one of id / text / label / role / ocrText — prefer id, which survives copy changes and localization. An ocrText selector OCRs the screen and taps the matched text's center. Needs the session smix_launch_app opens (SMIX_UDID env var set)."
    )]
    async fn smix_tap(
        &self,
        Parameters(params): Parameters<SelectorParams>,
    ) -> Result<CallToolResult, McpError> {
        let sel = params.to_selector()?;
        let app = self.app.lock().await;
        // OcrText bypasses the tree resolver: find the text's frame via
        // Apple Vision OCR and tap its normalized center (IOHID
        // synthesize), the same dispatch the maestro adapter uses.
        let outcome = match ocr_text_of(&sel) {
            // An OCR hit is a text frame, not a resolved element, so
            // there is nothing to have missed.
            Some(needle) => app
                .tap_by_text_ocr(needle, &[])
                .await
                .map(|()| smix_sdk::ActOutcome::unjudged()),
            None => app.tap(&sel).await,
        }
        .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
        // What the touch landed on, not just that one was sent. This
        // used to answer "tapped: <selector>" — a claim about the
        // selector, made on the evidence that a touch had been
        // synthesised somewhere, which is the reading a consumer found
        // out the hard way was not the same thing.
        //
        // The elements are listed even when the verdict passed: the
        // verdict cannot see an element covered by something else and
        // this list can, and an agent deciding whether to retry or to
        // report upward is exactly who needs to know.
        let mut report = format!("tapped: {}", smix_selector::describe_selector(&sel));
        if !outcome.observed.is_empty() {
            let at: Vec<String> = outcome
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
            report.push_str(&format!("\nthe tapped point is inside: {}", at.join(" < ")));
        }
        if let smix_sdk::ActVerdict::Unconfirmable(why) = &outcome.verdict {
            report.push_str(&format!("\nnot verified: {why}"));
        }
        Ok(CallToolResult::success(vec![Content::text(report)]))
    }

    #[tool(
        description = "Type text into a field. Names the field like smix_tap, except ocrText — an OCR hit is a text frame, not a focusable element. Tap the field first if it is not already focused. Needs the session smix_launch_app opens (SMIX_UDID env var set)."
    )]
    async fn smix_fill(
        &self,
        Parameters(params): Parameters<FillParams>,
    ) -> Result<CallToolResult, McpError> {
        let sel = params.target.to_selector()?;
        if ocr_text_of(&sel).is_some() {
            return Err(McpError::invalid_params(
                "ocrText cannot name a fill target — an OCR hit is a text frame on \
                 the screen, not a focusable accessibility element, so there is \
                 nothing to type into. Name the field with id / text / label / role; \
                 if the field is invisible to the accessibility tree, smix_tap it via \
                 ocrText to focus it, then smix_fill the field the tree does expose",
                None,
            ));
        }
        let app = self.app.lock().await;
        app.fill(&sel, &params.text)
            .await
            .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "filled {} with {:?}",
            smix_selector::describe_selector(&sel),
            params.text
        ))]))
    }

    #[tool(
        description = "Swipe once through the content. `direction` names what you want to see (down reveals what is below), not which way the finger moves. Needs the session smix_launch_app opens (SMIX_UDID env var set)."
    )]
    async fn smix_swipe(
        &self,
        Parameters(params): Parameters<SwipeParams>,
    ) -> Result<CallToolResult, McpError> {
        let dir = parse_direction(&params.direction)?;
        let app = self.app.lock().await;
        app.swipe_once(dir)
            .await
            .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "swiped: {}",
            params.direction
        ))]))
    }

    #[tool(
        description = "Swipe until an element comes into view, then stop. Use this rather than repeated swipes — it knows when to stop. Not for ocrText — swipe with smix_swipe and check with smix_find between swipes instead. Needs the session smix_launch_app opens (SMIX_UDID env var set)."
    )]
    async fn smix_scroll(
        &self,
        Parameters(params): Parameters<ScrollParams>,
    ) -> Result<CallToolResult, McpError> {
        let sel = params.target.to_selector()?;
        if ocr_text_of(&sel).is_some() {
            return Err(McpError::invalid_params(
                "ocrText cannot drive smix_scroll — its stop condition resolves \
                 against the accessibility tree, which never matches OCR text. \
                 Use smix_swipe to move through the content and smix_find with \
                 ocrText between swipes to know when to stop",
                None,
            ));
        }
        let dir = parse_direction(params.direction.as_deref().unwrap_or("down"))?;
        let app = self.app.lock().await;
        app.scroll(&sel, dir)
            .await
            .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "scrolled to: {}",
            smix_selector::describe_selector(&sel)
        ))]))
    }

    #[tool(
        description = "Launch an app by bundle id, or bring it to the front if it is running. Opens the runner session the other tools drive through — call this before smix_describe / smix_tap / etc. Requires the SMIX_UDID env var (set it in the MCP server config)."
    )]
    async fn smix_launch_app(
        &self,
        Parameters(params): Parameters<BundleParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut app = self.app.lock().await;
        if app.udid().is_none() {
            return Err(missing_udid_error());
        }
        app.launch(&params.bundle_id)
            .await
            .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
        // iOS driving requires a live runner session (v2 break #1). Bind
        // one to the launched bundle so the sense/act tools below drive
        // through it instead of the removed legacy per-request path.
        app.open_session_in_place(&params.bundle_id, true)
            .await
            .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "launched: {}",
            params.bundle_id
        ))]))
    }

    #[tool(
        description = "Terminate an app by bundle id. Already-stopped is a no-op success. Requires the SMIX_UDID env var (set it in the MCP server config)."
    )]
    async fn smix_stop_app(
        &self,
        Parameters(params): Parameters<BundleParams>,
    ) -> Result<CallToolResult, McpError> {
        let app = self.app.lock().await;
        if app.udid().is_none() {
            return Err(missing_udid_error());
        }
        // `simctl terminate` exits non-zero when the app is not running,
        // which would break the already-stopped-is-a-no-op promise above.
        // Tolerate the failure the way the SDK's own launch paths do
        // (`launch_app_with_options`: "terminate failure is tolerated —
        // the app may already be dead").
        match app.terminate(&params.bundle_id).await {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "stopped: {}",
                params.bundle_id
            ))])),
            Err(_) => Ok(CallToolResult::success(vec![Content::text(format!(
                "stopped: {} (was not running — no-op)",
                params.bundle_id
            ))])),
        }
    }

    #[tool(
        description = "Assert an element is on screen, waiting up to 5s. Fails with the visible elements and near-miss suggestions when it is not — paste that failure back to yourself to see what the screen actually had. An ocrText selector polls Apple Vision OCR on the same 5s budget. Needs the session smix_launch_app opens (SMIX_UDID env var set)."
    )]
    async fn smix_assert_visible(
        &self,
        Parameters(params): Parameters<SelectorParams>,
    ) -> Result<CallToolResult, McpError> {
        let sel = params.to_selector()?;
        let app = self.app.lock().await;
        match ocr_text_of(&sel) {
            // Same budget and cadence as the tree path: `App::assert_visible`
            // waits 5 s at the driver's 250 ms poll interval.
            Some(needle) => {
                let timeout = Duration::from_secs(5);
                let start = std::time::Instant::now();
                loop {
                    let hit = app
                        .find_by_text_ocr(needle, &[])
                        .await
                        .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
                    if hit.is_some() {
                        break;
                    }
                    if start.elapsed() >= timeout {
                        return Err(McpError::internal_error(
                            format!(
                                "expect.toBeVisible: not visible — {} (Apple Vision OCR \
                                 found no match within {}ms; check spelling / recognition \
                                 language / surface contrast)",
                                smix_selector::describe_selector(&sel),
                                timeout.as_millis()
                            ),
                            None,
                        ));
                    }
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
            }
            None => {
                app.assert_visible(&sel)
                    .await
                    .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
            }
        }
        Ok(CallToolResult::success(vec![Content::text(format!(
            "visible: {}",
            smix_selector::describe_selector(&sel)
        ))]))
    }

    #[tool(
        description = "Assert an element is NOT on screen (single probe, no waiting). An ocrText selector checks with one Apple Vision OCR pass. Needs the session smix_launch_app opens (SMIX_UDID env var set)."
    )]
    async fn smix_assert_not_visible(
        &self,
        Parameters(params): Parameters<SelectorParams>,
    ) -> Result<CallToolResult, McpError> {
        let sel = params.to_selector()?;
        let app = self.app.lock().await;
        match ocr_text_of(&sel) {
            // A tree-routed OcrText never matches, so this assert used to
            // pass vacuously — a false green. Probe OCR once, mirroring the
            // tree path's single non-waiting `find`.
            Some(needle) => {
                let hit = app
                    .find_by_text_ocr(needle, &[])
                    .await
                    .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
                if hit.is_some() {
                    return Err(McpError::internal_error(
                        format!(
                            "expect.toNotBeVisible: element is visible — {}",
                            smix_selector::describe_selector(&sel)
                        ),
                        None,
                    ));
                }
            }
            None => {
                app.assert_not_visible(&sel)
                    .await
                    .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
            }
        }
        Ok(CallToolResult::success(vec![Content::text(format!(
            "not visible: {}",
            smix_selector::describe_selector(&sel)
        ))]))
    }

    #[tool(
        description = "Press a named key (Return/Delete/Tab/Space/Escape/arrow keys). Needs the session smix_launch_app opens (SMIX_UDID env var set)."
    )]
    async fn smix_press_key(
        &self,
        Parameters(params): Parameters<PressKeyParams>,
    ) -> Result<CallToolResult, McpError> {
        let k = parse_key_name(&params.key).map_err(|m| McpError::invalid_params(m, None))?;
        let app = self.app.lock().await;
        app.press_key(k)
            .await
            .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "pressed: {}",
            params.key
        ))]))
    }

    #[tool(
        description = "Capture a base64-PNG screenshot of the current screen. Requires the SMIX_UDID env var (set it in the MCP server config)."
    )]
    async fn smix_screenshot(&self) -> Result<CallToolResult, McpError> {
        let app = self.app.lock().await;
        if app.udid().is_none() {
            return Err(missing_udid_error());
        }
        let png = app
            .screenshot()
            .await
            .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
        Ok(CallToolResult::success(vec![Content::text(b64)]))
    }
}

/// Read a swipe direction the way an agent writes one.
///
/// The value names what content to see, not the finger's path — both runners
/// map it to the inverse gesture, which is why "down" reveals what is below.
fn parse_direction(s: &str) -> Result<SwipeDirection, McpError> {
    match s.to_ascii_lowercase().as_str() {
        "up" => Ok(SwipeDirection::Up),
        "down" => Ok(SwipeDirection::Down),
        "left" => Ok(SwipeDirection::Left),
        "right" => Ok(SwipeDirection::Right),
        _ => Err(McpError::invalid_params(
            format!("unknown direction `{s}`; accepted: up, down, left, right"),
            None,
        )),
    }
}

fn parse_key_name(s: &str) -> Result<SdkKeyName, String> {
    match s {
        "return" | "Return" => Ok(KeyName::Return),
        "delete" | "Delete" => Ok(KeyName::Delete),
        "tab" | "Tab" => Ok(KeyName::Tab),
        "space" | "Space" => Ok(KeyName::Space),
        "escape" | "Escape" => Ok(KeyName::Escape),
        "arrowUp" | "ArrowUp" => Ok(KeyName::ArrowUp),
        "arrowDown" | "ArrowDown" => Ok(KeyName::ArrowDown),
        "arrowLeft" | "ArrowLeft" => Ok(KeyName::ArrowLeft),
        "arrowRight" | "ArrowRight" => Ok(KeyName::ArrowRight),
        other => Err(format!(
            "unknown key {other:?} — expected one of: return/delete/tab/space/escape/arrowUp/arrowDown/arrowLeft/arrowRight"
        )),
    }
}

#[tool_handler]
impl ServerHandler for SmixMcpService {
    fn get_info(&self) -> ServerInfo {
        let mut impl_info = Implementation::from_build_env();
        impl_info.name = "smix-mcp".into();
        impl_info.title = Some("smix (Rust)".into());
        impl_info.version = env!("CARGO_PKG_VERSION").into();

        let mut info = ServerInfo::default();
        info.protocol_version = ProtocolVersion::default();
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.server_info = impl_info;
        // The first thing an agent reads. It named smix_find_text and
        // smix_tap_text until those tools were generalized away — an
        // introduction advertising tools that no longer exist.
        info.instructions = Some(
            "smix drives an iOS Simulator. Call smix_launch_app first — it brings the \
             app to the front and opens the runner session the other tools drive \
             through. Then smix_describe to see what is on screen and learn the element \
             ids, smix_tap / smix_fill / smix_press_key to interact, and \
             smix_assert_visible to check. Name elements with exactly one of id / text / \
             label / role / ocrText — prefer id, which survives copy edits and \
             translation. Failures come back with near-miss suggestions and the \
             elements that were on screen; read them rather than guessing again. \
             The SMIX_UDID env var binds this server to one simulator and is \
             required — smix_launch_app and the session every other tool drives \
             through depend on it; set it in the MCP server config. \
             SMIX_RUNNER_PORT (default 22087) finds its runner."
                .into(),
        );
        info
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let port: u16 = std::env::var("SMIX_RUNNER_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(22087);

    // Lazy on purpose: the MCP client launches this server at ITS
    // startup, usually before anyone has run `smix runner up`. Dying
    // here left a dead server for the whole client session; now the
    // first tool call reports the runner story instead.
    let mut app = App::connect_to_runner_lazy(port);

    if let Ok(udid) = std::env::var("SMIX_UDID") {
        app = app.with_udid(udid);
    }

    let service = SmixMcpService::new(app);
    let server = service.serve(stdio()).await?;
    server.waiting().await?;
    Ok(())
}
