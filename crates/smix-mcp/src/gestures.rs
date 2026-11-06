//! Gestures past a tap: swipe, scroll, long press and drag.

use rmcp::ErrorData as McpError;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use smix_input::SwipeDirection;
use smix_mcp::{SelectorParams, chain_of, ocr_text_of, point_of};
use std::time::Duration;

use crate::SmixMcpService;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct SwipeParams {
    /// Which way to travel through the content: up, down, left, or right.
    /// This names what you want to SEE — "down" reveals what is below,
    /// whichever way the finger has to move to get there. Leave it out
    /// when giving swipe_from and swipe_to.
    direction: Option<String>,
    /// Where the finger starts, as "X%,Y%" or "X,Y" in 0..1. The
    /// authorised coordinate escape hatch for swipe, for screens with
    /// nothing nameable to swipe between. Needs swipe_to as well.
    swipe_from: Option<String>,
    /// Where the finger ends, same form as swipe_from.
    swipe_to: Option<String>,
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

/// What to press, and for how long.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct LongPressParams {
    /// What to press: exactly one of id / text / label / role / ocrText,
    /// or point ("X%,Y%" or "X,Y" in 0..1) for a place with nothing
    /// nameable, such as an empty spot on a home screen.
    target: SelectorParams,
    /// How long the finger stays down, in milliseconds. Defaults to 500.
    #[serde(default)]
    duration_ms: Option<u64>,
}

/// Where a drag starts and ends, and its timing.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct DragParams {
    /// Where the finger goes down, named like smix_tap's target, point
    /// included.
    from: SelectorParams,
    /// Where it lifts, same forms as from.
    to: SelectorParams,
    /// How long the finger stays still at the start before moving, in
    /// milliseconds (0..=10000, default 500). Long enough for a home
    /// screen icon or a launcher widget to come loose; on iOS a hold past
    /// about 700 ms opens the icon's menu first and the move then picks a
    /// menu item instead of carrying the icon.
    #[serde(default)]
    hold_ms: Option<u64>,
    /// How long the move takes, in milliseconds (50..=10000, default 800).
    #[serde(default)]
    duration_ms: Option<u64>,
    /// How long the finger rests at the end before lifting, in
    /// milliseconds (0..=5000, default 300), so a drop target such as
    /// "Remove" sees it.
    #[serde(default)]
    release_ms: Option<u64>,
}

fn failed(e: smix_error::ExpectationFailure) -> McpError {
    McpError::internal_error(e.to_prompt(), None)
}

#[tool_router(router = gesture_router, vis = "pub(crate)")]
impl SmixMcpService {
    #[tool(
        description = "Swipe once through the content. `direction` names what you want to see (down reveals what is below), not which way the finger moves. Needs the session smix_launch_app opens (SMIX_UDID env var set)."
    )]
    /// CLI: smix swipe
    async fn smix_swipe(
        &self,
        Parameters(params): Parameters<SwipeParams>,
    ) -> Result<CallToolResult, McpError> {
        let app = self.bound_app().await?;
        // A direction and a pair of points are two different asks, and
        // answering one when both arrived is how a flow does something
        // nobody wrote.
        match (params.direction, params.swipe_from, params.swipe_to) {
            (Some(direction), None, None) => {
                let dir = parse_direction(&direction)?;
                app.swipe_once(dir)
                    .await
                    .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
                Ok(CallToolResult::success(vec![ContentBlock::text(format!(
                    "swiped: {direction}"
                ))]))
            }
            (None, Some(from), Some(to)) => {
                let from_pt = smix_selector::point_from_str(&from)
                    .map_err(|why| McpError::invalid_params(format!("swipe_from: {why}"), None))?;
                let to_pt = smix_selector::point_from_str(&to)
                    .map_err(|why| McpError::invalid_params(format!("swipe_to: {why}"), None))?;
                app.swipe_at_coord(from_pt, to_pt)
                    .await
                    .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
                Ok(CallToolResult::success(vec![ContentBlock::text(format!(
                    "swiped: ({:.3}, {:.3}) → ({:.3}, {:.3})",
                    from_pt.0, from_pt.1, to_pt.0, to_pt.1
                ))]))
            }
            (None, None, None) => Err(McpError::invalid_params(
                "swipe needs either a direction or both swipe_from and swipe_to".to_string(),
                None,
            )),
            (None, _, _) => Err(McpError::invalid_params(
                "a coordinate swipe needs both ends: swipe_from and swipe_to".to_string(),
                None,
            )),
            (Some(_), _, _) => Err(McpError::invalid_params(
                "swipe takes a direction or a swipe_from/swipe_to pair, not both".to_string(),
                None,
            )),
        }
    }

    #[tool(
        description = "Swipe until an element is wholly in view, then stop. Use this rather than repeated swipes — it knows when to stop. Takes id / text / label / role / ocrText or a fallback chain of them: after each swipe it reads the tree, then each ocrText in the chain's order. Needs the session smix_launch_app opens (SMIX_UDID env var set)."
    )]
    /// CLI: smix scroll
    async fn smix_scroll(
        &self,
        Parameters(params): Parameters<ScrollParams>,
    ) -> Result<CallToolResult, McpError> {
        let sel = params.target.to_selector()?;
        if point_of(&sel).is_some() {
            return Err(McpError::invalid_params(
                "a point is already a place on this screen; scrolling to it means nothing — the schema says so and now this does too. Name the \
                 element with id / text / label / role / ocrText, or tap the \
                 point and act on what the tap put on screen.",
                None,
            ));
        }
        // A chain and an `ocrText` are both looked for after every swipe:
        // the scroll loop reads the tree and then each `ocrText` in the
        // order the chain names them. A point inside a chain is still a
        // place rather than something a swipe can bring into view.
        if let Some(layers) = chain_of(&sel) {
            for (i, layer) in layers.iter().enumerate() {
                if point_of(layer).is_some() {
                    return Err(McpError::invalid_params(
                        format!(
                            "fallback[{i}] is a point, and a point is a place rather \
                             than something a scroll can bring into view. Only smix_tap takes one"
                        ),
                        None,
                    ));
                }
            }
        }
        let dir = parse_direction(params.direction.as_deref().unwrap_or("down"))?;
        let app = self.bound_app().await?;
        app.scroll(&sel, dir, &smix_sdk::ScrollUntil::default())
            .await
            .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "scrolled to: {}",
            smix_selector::describe_selector(&sel)
        ))]))
    }

    #[tool(
        description = "Press and hold an element or a point. For what opens on a long press: a context menu, a home screen's edit mode, a launcher widget's resize frame. Name the target like smix_tap, or give point for a place with nothing nameable. durationMs defaults to 500. Needs the session smix_launch_app opens."
    )]
    /// CLI: smix long-press
    async fn smix_long_press(
        &self,
        Parameters(params): Parameters<LongPressParams>,
    ) -> Result<CallToolResult, McpError> {
        let sel = params.target.to_selector()?;
        if chain_of(&sel).is_some() {
            return Err(McpError::invalid_params(
                "a long press is one touch in one place; name one element or point, not a fallback chain",
                None,
            ));
        }
        let ms = params.duration_ms.unwrap_or(500);
        let app = self.bound_app().await?;
        if point_of(&sel).is_some() || ocr_text_of(&sel).is_some() {
            let (nx, ny) = app.locate(&sel).await.map_err(failed)?;
            app.long_press_at_coord(nx, ny, ms).await.map_err(failed)?;
        } else {
            app.long_press(&sel, Duration::from_millis(ms))
                .await
                .map_err(failed)?;
        }
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "long-pressed {} for {ms} ms",
            smix_selector::describe_selector(&sel)
        ))]))
    }

    #[tool(
        description = "Press, hold, move and lift as one touch: drag a home screen icon or widget, move a launcher widget to Remove, reorder a list. from and to are each named like smix_tap's target or a point. The finger holds still holdMs (default 500), moves for durationMs (default 800), rests releaseMs (default 300), then lifts. Needs the session smix_launch_app opens."
    )]
    /// CLI: smix drag
    async fn smix_drag(
        &self,
        Parameters(params): Parameters<DragParams>,
    ) -> Result<CallToolResult, McpError> {
        let from = params.from.to_selector()?;
        let to = params.to.to_selector()?;
        let app = self.bound_app().await?;
        let d = app
            .drag_between(
                &from,
                &to,
                params.hold_ms.unwrap_or(smix_input::DRAG_DEFAULT_HOLD_MS),
                params
                    .duration_ms
                    .unwrap_or(smix_input::DRAG_DEFAULT_DURATION_MS),
                params
                    .release_ms
                    .unwrap_or(smix_input::DRAG_DEFAULT_RELEASE_MS),
            )
            .await
            .map_err(failed)?;
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "dragged ({:.3}, {:.3}) → ({:.3}, {:.3}): held {} ms, moved {} ms, rested {} ms",
            d.from.0, d.from.1, d.to.0, d.to.1, d.hold_ms, d.duration_ms, d.release_ms
        ))]))
    }
}

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
