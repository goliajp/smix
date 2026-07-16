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
use smix_input::KeyName;
use smix_sdk::{App, KeyName as SdkKeyName, Selector, text};
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

#[derive(Debug, Deserialize, JsonSchema)]
struct TextSelectorParams {
    /// Literal text to match; case-insensitive, scanned across
    /// label / title / value.
    text: String,
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
        description = "Check whether a text selector matches any element (boolean quick-probe)."
    )]
    async fn smix_find_text(
        &self,
        Parameters(params): Parameters<TextSelectorParams>,
    ) -> Result<CallToolResult, McpError> {
        let sel: Selector = text(&params.text);
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
        description = "Tap an element by text selector (host-side resolve + Apple native event chain)."
    )]
    async fn smix_tap_text(
        &self,
        Parameters(params): Parameters<TextSelectorParams>,
    ) -> Result<CallToolResult, McpError> {
        let sel: Selector = text(&params.text);
        let app = self.app.lock().await;
        app.tap(&sel)
            .await
            .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "tapped: {}",
            params.text
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
        info.instructions = Some(
            "smix — iOS Simulator automation. Use smix_describe to see the current screen, then smix_find_text / smix_tap_text / smix_press_key to interact. SMIX_UDID env var binds the server to a specific simulator; SMIX_RUNNER_PORT (default 22087) selects the SmixRunner endpoint."
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
