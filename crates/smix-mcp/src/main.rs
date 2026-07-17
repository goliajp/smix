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
use smix_mcp::SelectorParams;
use smix_sdk::{App, KeyName as SdkKeyName};
use std::sync::Arc;
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

#[tool_router]
impl SmixMcpService {
    fn new(app: App) -> Self {
        Self {
            app: Arc::new(Mutex::new(app)),
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Get a structured description of the current screen — visible elements + bounds."
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

    #[tool(description = "Get the raw A11yNode tree of the current screen.")]
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
        description = "Check whether an element is on screen, as a plain true/false. Use this to look before you act; use smix_assert_visible when absence should be a failure."
    )]
    async fn smix_find(
        &self,
        Parameters(params): Parameters<SelectorParams>,
    ) -> Result<CallToolResult, McpError> {
        let sel = params.to_selector()?;
        let app = self.app.lock().await;
        let exists = app
            .find(&sel)
            .await
            .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
        Ok(CallToolResult::success(vec![Content::text(
            if exists { "true" } else { "false" }.to_string(),
        )]))
    }

    #[tool(
        description = "Tap an element. Name it with exactly one of id / text / label / role / ocrText — prefer id, which survives copy changes and localization."
    )]
    async fn smix_tap(
        &self,
        Parameters(params): Parameters<SelectorParams>,
    ) -> Result<CallToolResult, McpError> {
        let sel = params.to_selector()?;
        let app = self.app.lock().await;
        app.tap(&sel)
            .await
            .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "tapped: {}",
            smix_selector::describe_selector(&sel)
        ))]))
    }

    #[tool(
        description = "Type text into a field. Names the field the same way smix_tap does. Tap the field first if it is not already focused."
    )]
    async fn smix_fill(
        &self,
        Parameters(params): Parameters<FillParams>,
    ) -> Result<CallToolResult, McpError> {
        let sel = params.target.to_selector()?;
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
        description = "Swipe once through the content. `direction` names what you want to see (down reveals what is below), not which way the finger moves."
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
        description = "Swipe until an element comes into view, then stop. Use this rather than repeated swipes — it knows when to stop."
    )]
    async fn smix_scroll(
        &self,
        Parameters(params): Parameters<ScrollParams>,
    ) -> Result<CallToolResult, McpError> {
        let sel = params.target.to_selector()?;
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

    #[tool(description = "Launch an app by bundle id, or bring it to the front if it is running.")]
    async fn smix_launch_app(
        &self,
        Parameters(params): Parameters<BundleParams>,
    ) -> Result<CallToolResult, McpError> {
        let app = self.app.lock().await;
        app.launch(&params.bundle_id)
            .await
            .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "launched: {}",
            params.bundle_id
        ))]))
    }

    #[tool(description = "Terminate an app by bundle id. Already-stopped is a no-op success.")]
    async fn smix_stop_app(
        &self,
        Parameters(params): Parameters<BundleParams>,
    ) -> Result<CallToolResult, McpError> {
        let app = self.app.lock().await;
        app.terminate(&params.bundle_id)
            .await
            .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "stopped: {}",
            params.bundle_id
        ))]))
    }

    #[tool(
        description = "Assert an element is on screen. Fails with the visible elements and near-miss suggestions when it is not — paste that failure back to yourself to see what the screen actually had."
    )]
    async fn smix_assert_visible(
        &self,
        Parameters(params): Parameters<SelectorParams>,
    ) -> Result<CallToolResult, McpError> {
        let sel = params.to_selector()?;
        let app = self.app.lock().await;
        app.assert_visible(&sel)
            .await
            .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "visible: {}",
            smix_selector::describe_selector(&sel)
        ))]))
    }

    #[tool(description = "Assert an element is NOT on screen.")]
    async fn smix_assert_not_visible(
        &self,
        Parameters(params): Parameters<SelectorParams>,
    ) -> Result<CallToolResult, McpError> {
        let sel = params.to_selector()?;
        let app = self.app.lock().await;
        app.assert_not_visible(&sel)
            .await
            .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "not visible: {}",
            smix_selector::describe_selector(&sel)
        ))]))
    }

    #[tool(description = "Press a named key (Return/Delete/Tab/Space/Escape/arrow keys).")]
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
        description = "Capture a base64-PNG screenshot of the current screen (requires SMIX_UDID)."
    )]
    async fn smix_screenshot(&self) -> Result<CallToolResult, McpError> {
        let app = self.app.lock().await;
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
            "smix drives an iOS Simulator. Call smix_describe first to see what is on \
             screen and learn the element ids, then smix_tap / smix_fill / smix_press_key \
             to interact and smix_assert_visible to check. Name elements with exactly one \
             of id / text / label / role / ocrText — prefer id, which survives copy edits \
             and translation. Failures come back with near-miss suggestions and the \
             elements that were on screen; read them rather than guessing again. \
             SMIX_UDID binds this server to one simulator; SMIX_RUNNER_PORT (default \
             22087) finds its runner."
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

    let mut app = App::connect_to_runner(port)
        .await
        .map_err(|e| format!("connect to runner on port {port} failed: {}", e.to_prompt()))?;

    if let Ok(udid) = std::env::var("SMIX_UDID") {
        app = app.with_udid(udid);
    }

    let service = SmixMcpService::new(app);
    let server = service.serve(stdio()).await?;
    server.waiting().await?;
    Ok(())
}
