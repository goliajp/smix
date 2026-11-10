//! Frames of the screen, written to files: an agent's tool result has a
//! size limit and a full phone screenshot as base64 text is several times
//! over it (an iPhone frame came back as 775,008 characters and was cut).

use base64::Engine as _;
use rmcp::ErrorData as McpError;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use smix_mcp::{SelectorParams, chain_of, ocr_text_of, point_of};
use std::path::PathBuf;

use crate::{SmixMcpService, first_visible_layer, missing_udid_error};

/// Where the frame goes and whether a small copy comes back too.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ScreenshotParams {
    /// Write the PNG here instead of a new file under the system's
    /// temporary directory.
    #[serde(default)]
    path: Option<String>,
    /// Also return a copy scaled down to this many pixels wide, as an
    /// image. Left out, only the file is written.
    #[serde(default)]
    max_width: Option<u32>,
}

/// How much of the screen `smix_describe` gives back.
#[derive(Debug, Default, Deserialize, JsonSchema)]
struct DescribeParams {
    /// One line per element — role, name, id — and no bounds: a fraction
    /// of the size, enough to write the next selector.
    #[serde(default)]
    compact: bool,
}

/// Write `png` to `path`, or to a new file under the temporary directory,
/// and say where and how big.
fn keep(png: &[u8], path: Option<&str>) -> Result<String, McpError> {
    let target = match path {
        Some(p) => PathBuf::from(p),
        None => {
            let dir = std::env::temp_dir().join("smix-screenshots");
            std::fs::create_dir_all(&dir).map_err(|e| {
                McpError::internal_error(format!("create {}: {e}", dir.display()), None)
            })?;
            let ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis());
            dir.join(format!("screen-{ms}.png"))
        }
    };
    std::fs::write(&target, png)
        .map_err(|e| McpError::internal_error(format!("write {}: {e}", target.display()), None))?;
    let size = smix_sdk::screenshot_shrink::png_size(png).map_or_else(
        || "size unknown".to_string(),
        |(w, h)| format!("{w}×{h} px"),
    );
    Ok(format!(
        "screenshot: {} ({size}, {} bytes)",
        target.display(),
        png.len()
    ))
}

#[tool_router(router = frame_router, vis = "pub(crate)")]
impl SmixMcpService {
    #[tool(
        description = "Describe the current screen: the visible elements a reader could name, each with its role, name, id and bounds, as JSON. Give compact: true for one line per element — role, name, id — without bounds, a fraction of the size. Needs smix_use, then the session smix_launch_app opens."
    )]
    /// CLI: smix describe
    async fn smix_describe(
        &self,
        Parameters(params): Parameters<DescribeParams>,
    ) -> Result<CallToolResult, McpError> {
        let app = self.bound_app().await?;
        let desc = app
            .describe()
            .await
            .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
        let text = if params.compact {
            desc.compact()
        } else {
            serde_json::to_string_pretty(&desc).unwrap_or_default()
        };
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }

    #[tool(
        description = "Tap an element and take a frame in the same call, in that order. For UI that does not wait: something that hides itself after a few seconds outlives neither a second tool call nor the turn between them. Name the element with exactly one of id / text / label / role — prefer id. Returns a line saying where the frame came from and how many milliseconds after the tap it landed, and the file the PNG was written to. A tap that fails returns no frame. Needs the session smix_launch_app opens."
    )]
    /// CLI: smix tap --then-screenshot
    async fn smix_tap_then_screenshot(
        &self,
        Parameters(params): Parameters<SelectorParams>,
    ) -> Result<CallToolResult, McpError> {
        let sel = params.to_selector()?;
        let app = self.bound_app().await?;
        // A chain is dispatched by the caller — handing the whole thing
        // to the resolver is one no where several tries were asked for.
        // Walk it, take the first layer that is on screen, and tap that
        // one; the frame is then a picture of a tap that resolved.
        let target = match chain_of(&sel) {
            Some(layers) => match first_visible_layer(&app, &layers).await? {
                Some(i) => layers[i].clone(),
                None => {
                    return Err(McpError::internal_error(
                        "no layer of the chain is on screen, so nothing was \
                         tapped and no frame was taken"
                            .to_string(),
                        None,
                    ));
                }
            },
            None => sel.clone(),
        };
        // Neither form resolves to an element in the tree, and this tool
        // reports where the touch landed — so it refuses them by name
        // rather than reporting a landing it did not observe. Checked on
        // the layer that won, because that is the one being tapped.
        if point_of(&target).is_some() || ocr_text_of(&target).is_some() {
            return Err(McpError::invalid_params(
                "smix_tap_then_screenshot needs a selector the tree can \
                 resolve, and this one is a point or an ocrText hit. Both \
                 are dispatched without resolving a target, so there would \
                 be nothing to say about where the touch landed — use \
                 smix_tap followed by smix_screenshot if that is what you \
                 want."
                    .to_string(),
                None,
            ));
        }
        let (outcome, captured) = app
            .tap_then_capture(&target)
            .await
            .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
        let mut said = format!(
            "tapped — frame via {} {} ms later, {} bytes",
            captured.via,
            captured.gap_ms,
            captured.png.len()
        );
        if let smix_sdk::ActVerdict::Unconfirmable(why) = &outcome.verdict {
            said.push_str(&format!("\nnot verified: {why}"));
        }
        let written = keep(&captured.png, None)?;
        said.push('\n');
        said.push_str(&written);
        Ok(CallToolResult::success(vec![ContentBlock::text(said)]))
    }

    #[tool(
        description = "Capture the current screen to a PNG file and return its path and size, not the image: a full phone frame is larger than a tool result may hold. Open the file to look at it. Give maxWidth (pixels) to also get a copy scaled to that width inline, e.g. 400 to read a layout; give path to choose where the file goes. Needs a device bound with smix_use."
    )]
    /// CLI: smix sim screenshot
    async fn smix_screenshot(
        &self,
        Parameters(params): Parameters<ScreenshotParams>,
    ) -> Result<CallToolResult, McpError> {
        let app = self.bound_app().await?;
        if app.udid().is_none() {
            return Err(missing_udid_error());
        }
        let png = app
            .screenshot()
            .await
            .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
        let mut said = keep(&png, params.path.as_deref())?;
        let mut out = Vec::new();
        if let Some(width) = params.max_width {
            let small = smix_sdk::screenshot_shrink::shrink_to_width(&png, width)
                .map_err(|e| McpError::invalid_params(e.to_prompt(), None))?;
            said.push_str(&format!(
                "\ninline copy: {}×{} px, {} bytes",
                small.width,
                small.height,
                small.png.len()
            ));
            out.push(ContentBlock::image(
                base64::engine::general_purpose::STANDARD.encode(&small.png),
                "image/png",
            ));
        }
        out.insert(0, ContentBlock::text(said));
        Ok(CallToolResult::success(out))
    }
}
