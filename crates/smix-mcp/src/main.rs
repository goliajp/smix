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
use smix_mcp::{SelectorParams, chain_of, ocr_locales_of, ocr_text_of, point_of};
use smix_sdk::{App, KeyName as SdkKeyName};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

#[derive(Clone)]
struct SmixMcpService {
    /// The App every sense/act tool drives through. Replaced, not
    /// reconfigured, when `smix_use` changes device: an App is bound to a
    /// port at construction.
    app: Arc<Mutex<App>>,
    /// Which device this session chose, if any.
    ///
    /// Separate from `app` because the answer "none yet" has to be
    /// tellable. An App exists either way, and asking it produces a
    /// connection error that describes a symptom rather than the choice
    /// nobody made.
    session: Arc<smix_mcp::SessionState>,
    /// Tool router populated by #[tool_router] macro; read by the
    /// macro-generated `serve` plumbing, not by hand-written code.
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

/// Which device to drive, and optionally on which port and with which app.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct UseParams {
    /// Simulator UDID, as reported by `smix_devices`.
    udid: String,
    /// Runner port. Defaults to 22087; give a different one to drive two
    /// devices at once from separate sessions.
    #[serde(default)]
    port: Option<u16>,
    /// Bundle id the runner latches its XCUIApplication to. The runner
    /// refuses to start without one unless the caller opts into a default.
    #[serde(default)]
    bundle_id: Option<String>,
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

/// Is any layer of a chain on screen, and which.
///
/// Every layer goes through the same split the single-selector path
/// takes, because a layer may be an `ocrText` and that never matches in
/// the tree. Not iterating — handing the chain to `App::find` — is one
/// no instead of several tries, and it reads exactly like the thing not
/// being there.
async fn first_visible_layer(
    app: &smix_sdk::App,
    layers: &[smix_selector::Selector],
) -> Result<Option<usize>, McpError> {
    for (i, layer) in layers.iter().enumerate() {
        let seen = match ocr_text_of(layer) {
            Some(needle) => app
                .find_by_text_ocr(needle, ocr_locales_of(layer))
                .await
                .map_err(|e| McpError::internal_error(e.to_prompt(), None))?
                .is_some(),
            None => app
                .find(layer)
                .await
                .map_err(|e| McpError::internal_error(e.to_prompt(), None))?,
        };
        if seen {
            return Ok(Some(i));
        }
    }
    Ok(None)
}

#[tool_router]
impl SmixMcpService {
    fn new(app: App, session: smix_mcp::SessionState) -> Self {
        Self {
            app: Arc::new(Mutex::new(app)),
            session: Arc::new(session),
            tool_router: Self::tool_router(),
        }
    }

    /// The App, once a device has been chosen for this session.
    ///
    /// Every sense and act tool goes through here so that "no device
    /// bound" is answered by naming `smix_use`, rather than by whatever
    /// connection error the first call happens to produce.
    async fn bound_app(&self) -> Result<tokio::sync::MutexGuard<'_, App>, McpError> {
        self.session
            .require()
            .map_err(|hint| McpError::invalid_params(hint, None))?;
        Ok(self.app.lock().await)
    }

    /// Same, mutably.
    async fn bound_app_mut(&self) -> Result<tokio::sync::MutexGuard<'_, App>, McpError> {
        self.bound_app().await
    }

    // --- device lifecycle -------------------------------------------
    //
    // These exist so a conversation can start from nothing. Without them
    // the device came from `SMIX_UDID` in the client's config file — set
    // before anyone said a word, unchangeable without a restart — and the
    // runner had to be brought up by a human in another terminal.

    #[tool(
        description = "List the simulators available to drive, with their UDID, name and state. Call this first when no device is bound; pass one of the UDIDs to smix_use."
    )]
    /// CLI: smix sim list
    async fn smix_devices(&self) -> Result<CallToolResult, McpError> {
        let simctl = smix_simctl::SimctlClient::new();
        let devices = simctl
            .list_devices()
            .await
            .map_err(|e| McpError::internal_error(format!("simctl: {e}"), None))?;
        let listed: Vec<_> = devices
            .iter()
            .filter(|d| d.is_available)
            .map(|d| {
                serde_json::json!({
                    "udid": d.udid,
                    "name": d.name,
                    "state": d.state,
                })
            })
            .collect();
        let json = serde_json::to_string_pretty(&listed).unwrap_or_default();
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        description = "Bind this session to a simulator and bring its runner up, booting the device if needed. Everything else drives whatever this last chose. Call it again with another UDID to switch."
    )]
    /// CLI: smix runner up
    async fn smix_use(
        &self,
        Parameters(params): Parameters<UseParams>,
    ) -> Result<CallToolResult, McpError> {
        let port = params.port.unwrap_or(22087);

        // Already here: bringing the runner up again would restart the app
        // and drop whatever state the conversation had built up.
        //
        if let Some(current) = self.session.current()
            && current.udid == params.udid
            && current.port == port
            // health-decider: whether this server is already driving the
            // device the caller named, and can go on driving it.
            && smix_capsule::health_ok(port)
        {
            // `/health` answering is not the same as the session working:
            // it is a closure over a boot date and never touches the app
            // binding, so a reinstall leaves it saying 200 while every
            // real call fails. Saying "already driving" there hands the
            // agent a device it cannot drive.
            // Named when the caller named one. This tool is the case the
            // naming was written for: the session on this port may have
            // been brought up by somebody else, so "some app is drivable"
            // and "the app you asked for is drivable" are not the same
            // answer.
            if let Some(why) =
                smix_capsule::runner::probe_session_for(port, params.bundle_id.as_deref())
                    .unusable_because()
            {
                return Err(McpError::internal_error(
                    format!(
                        "the runner on port {port} answers /health, but its session \
                         is not usable: {why}. That happens when the app is \
                         reinstalled or terminated out from under the runner.\n\
                         Recover it in place, then use this tool again:\n  \
                         smix runner cycle"
                    ),
                    None,
                ));
            }
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "already driving {} on port {port}",
                params.udid
            ))]));
        }

        let simctl = smix_simctl::SimctlClient::new();
        if let Err(e) = simctl.boot(&params.udid).await
            && !e.to_string().contains("current state: Booted")
        {
            return Err(McpError::internal_error(
                format!("boot {}: {e}", params.udid),
                None,
            ));
        }

        let root = std::env::current_dir()
            .map_err(|e| McpError::internal_error(format!("cwd: {e}"), None))?;
        // Blocking: `up` spawns xcodebuild and waits on /health, and the
        // rmcp handler is async, so it goes to a blocking thread rather
        // than stalling the runtime for the length of a build.
        let udid = params.udid.clone();
        let bundle = params.bundle_id.clone();
        tokio::task::spawn_blocking(move || {
            smix_capsule::runner::up(
                &root,
                &udid,
                port,
                bundle.as_deref(),
                None,
                smix_capsule::runner::UpOptions {
                    record_enabled: false,
                    supervise: false,
                    attach_without_relaunch: false,
                    force_recover: false,
                },
            )
        })
        .await
        .map_err(|e| McpError::internal_error(format!("runner up panicked: {e}"), None))?
        .map_err(|e| McpError::internal_error(format!("runner up: {e}"), None))?;

        // Rebuild rather than reconfigure: an App is bound to its port at
        // construction, so switching device means a new one.
        let mut next = App::connect_to_runner_lazy(port);
        next = next.with_udid(params.udid.clone());

        // A runner on a port is not yet something the driving tools can
        // use: iOS driving happens inside a session, and without one every
        // sense and act call fails asking for `App::open_session`. Binding
        // a device and then leaving the caller a second, undiscoverable
        // step is the shape this tool exists to remove.
        // Announce the binding, so a `smix run` or a destructive CLI
        // command aimed at the same device is refused rather than landing
        // in the middle of an agent's session. Best-effort in one
        // direction only: a workspace root that cannot be found means
        // there is no ledger to write, which is the state every caller
        // was in before leases existed — but an actual refusal is
        // surfaced, because that one means somebody else is working.
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        // The ledger no longer depends on standing in a workspace. It
        // used to: no `.smix` above the working directory meant no lease
        // written, and this server is what held the lease that went
        // missing — a runner on port 22087, recorded in a tree nobody
        // else was standing in. The tree still decides where a dead
        // holder's build products would be settled, which is all it ever
        // decided.
        let root = smix_capsule::runner::workspace_root(&cwd).unwrap_or(cwd);
        if let Some(leases) = smix_lease::store::LeaseDir::machine() {
            match next.hold_device_lease(
                &root,
                &leases,
                &params.udid,
                &smix_capsule::reconcile::Reconciler,
            ) {
                Ok(settled) => {
                    for report in settled {
                        eprintln!("settled first: {}", report.line);
                    }
                }
                Err(e) => return Err(McpError::internal_error(e.to_string(), None)),
            }
        }

        if let Some(bundle) = params.bundle_id.as_deref() {
            next.open_session_in_place(bundle, true)
                .await
                .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
        }
        *self.app.lock().await = next;

        self.session.bind(smix_mcp::Bound {
            udid: params.udid.clone(),
            port,
        });
        Ok(CallToolResult::success(vec![Content::text(format!(
            "driving {} on port {port}",
            params.udid
        ))]))
    }

    #[tool(
        description = "Take the runner down and unbind this session's device. Leaves the simulator booted — shutting down a device someone else may be using is not this tool's call."
    )]
    /// CLI: smix runner down
    async fn smix_release(&self) -> Result<CallToolResult, McpError> {
        let Some(bound) = self.session.release() else {
            return Ok(CallToolResult::success(vec![Content::text(
                "nothing was bound".to_string(),
            )]));
        };
        let root = std::env::current_dir()
            .map_err(|e| McpError::internal_error(format!("cwd: {e}"), None))?;
        let port = bound.port;
        // No, and emphatically: an MCP release runs without anyone
        // watching. Ending another session's runner from here would be
        // the same accident as before, with nobody present to notice.
        tokio::task::spawn_blocking(move || smix_capsule::runner::down(&root, port))
            .await
            .map_err(|e| McpError::internal_error(format!("runner down panicked: {e}"), None))?
            .map_err(|e| McpError::internal_error(format!("runner down: {e}"), None))?;
        // Give the device back too. `runner down` ends the session; the
        // lease is what told everybody else the device was taken, and
        // leaving it behind would keep the next `smix run` waiting on a
        // session that has already ended.
        if let Err(e) = self.app.lock().await.release_device_lease() {
            eprintln!("warning: device lease not released: {e}");
        }
        Ok(CallToolResult::success(vec![Content::text(format!(
            "released {} on port {port}",
            bound.udid
        ))]))
    }

    #[tool(
        description = "Get a structured description of the current screen — visible elements + bounds. Needs the session smix_launch_app opens (SMIX_UDID env var set)."
    )]
    /// CLI: smix describe
    async fn smix_describe(&self) -> Result<CallToolResult, McpError> {
        let app = self.bound_app().await?;
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
    /// CLI: smix tree
    async fn smix_tree(&self) -> Result<CallToolResult, McpError> {
        let app = self.bound_app().await?;
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
    /// CLI: smix find
    async fn smix_find(
        &self,
        Parameters(params): Parameters<SelectorParams>,
    ) -> Result<CallToolResult, McpError> {
        let sel = params.to_selector()?;
        if point_of(&sel).is_some() {
            return Err(McpError::invalid_params(
                "a point names a place, not an element, so there is nothing here to \
                 find. Only smix_tap takes one. Name the element with id / text / \
                 label / role / ocrText, or tap the point and check what the tap \
                 landed on.",
                None,
            ));
        }
        let app = self.bound_app().await?;
        if let Some(layers) = chain_of(&sel) {
            for (i, layer) in layers.iter().enumerate() {
                if point_of(layer).is_some() {
                    return Err(McpError::invalid_params(
                        format!(
                            "fallback[{i}] is a point, and a point is a place rather \
                             than something that can be seen. Only smix_tap takes one"
                        ),
                        None,
                    ));
                }
            }
            let hit = first_visible_layer(&app, &layers).await?;
            return Ok(CallToolResult::success(vec![Content::text(match hit {
                Some(i) => format!(
                    "true — fallback[{i}] {}",
                    smix_selector::describe_selector(&layers[i])
                ),
                None => format!("false — all {} fallback layers missed", layers.len()),
            })]));
        }
        // The tree resolver never matches OcrText (live-vision op, not a
        // tree predicate) — routed through `app.find` an ocrText selector
        // is always false. Dispatch it to the OCR path instead, as the
        // maestro adapter does.
        let exists = match ocr_text_of(&sel) {
            Some(needle) => app
                .find_by_text_ocr(needle, ocr_locales_of(&sel))
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
    /// CLI: smix tap
    async fn smix_tap(
        &self,
        Parameters(params): Parameters<SelectorParams>,
    ) -> Result<CallToolResult, McpError> {
        let sel = params.to_selector()?;
        let app = self.bound_app().await?;
        // OcrText bypasses the tree resolver: find the text's frame via
        // Apple Vision OCR and tap its normalized center (IOHID
        // synthesize), the same dispatch the maestro adapter uses.
        // A chain is tried a layer at a time, first hit wins — the shape
        // `fallback:` has in a flow. Handing the whole chain to the
        // resolver is one no instead of several tries.
        if let Some(layers) = chain_of(&sel) {
            for (i, layer) in layers.iter().enumerate() {
                let hit = match (point_of(layer), ocr_text_of(layer)) {
                    (Some((nx, ny)), _) => app.tap_at_coord(nx, ny).await.map(|()| true),
                    (_, Some(needle)) => match app
                        .find_by_text_ocr(needle, ocr_locales_of(layer))
                        .await
                    {
                        Ok(Some(f)) => app.tap_at_coord(f.mid_x(), f.mid_y()).await.map(|()| true),
                        Ok(None) => Ok(false),
                        Err(e) => Err(e),
                    },
                    _ => match app.find(layer).await {
                        Ok(true) => app.tap(layer).await.map(|_| true),
                        Ok(false) => Ok(false),
                        Err(e) => Err(e),
                    },
                }
                .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
                if hit {
                    return Ok(CallToolResult::success(vec![Content::text(format!(
                        "tapped: fallback[{i}] {} — not verified: a chain reports \
                         which layer answered, not what the touch landed on",
                        smix_selector::describe_selector(layer)
                    ))]));
                }
            }
            return Err(McpError::internal_error(
                format!("all {} fallback layers missed", layers.len()),
                None,
            ));
        }
        let outcome = match (point_of(&sel), ocr_text_of(&sel)) {
            // A place, not a thing: the resolver has nothing to match, so
            // the touch is synthesised straight at the coordinate — the
            // same dispatch `tapOn: { point: … }` takes in a flow.
            (Some((nx, ny)), _) => app
                .tap_at_coord(nx, ny)
                .await
                .map(|()| smix_sdk::ActOutcome::unjudged()),
            // An OCR hit is a text frame, not a resolved element, so
            // there is nothing to have missed.
            (_, Some(needle)) => app
                .tap_by_text_ocr(needle, ocr_locales_of(&sel))
                .await
                .map(|()| smix_sdk::ActOutcome::unjudged()),
            _ => app.tap(&sel).await,
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
        description = "Type text into a field, replacing what it holds. Names the field like smix_tap, except ocrText — an OCR hit is a text frame, not a focusable element. Tap the field first if it is not already focused. Filling the same field twice leaves the second value, not both. Needs the session smix_launch_app opens (SMIX_UDID env var set)."
    )]
    /// CLI: smix fill
    async fn smix_fill(
        &self,
        Parameters(params): Parameters<FillParams>,
    ) -> Result<CallToolResult, McpError> {
        let sel = params.target.to_selector()?;
        if chain_of(&sel).is_some() {
            return Err(McpError::invalid_params(
                "typing needs one field, and a chain is several ways to name one thing. Find it with smix_find first, then fill what you found",
                None,
            ));
        }
        if point_of(&sel).is_some() {
            return Err(McpError::invalid_params(
                "a point names a place, not a field with a value — the schema says so and now this does too. Name the \
                 element with id / text / label / role / ocrText, or tap the \
                 point and act on what the tap put on screen.",
                None,
            ));
        }
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
        let app = self.bound_app().await?;
        app.fill(&sel, &params.text)
            .await
            .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
        // The value never comes back out — see `cmd_fill` in smix-cli
        // for why the length is the whole confirmation. This surface is
        // the more exposed of the two: an MCP result is written into the
        // conversation by construction.
        Ok(CallToolResult::success(vec![Content::text(format!(
            "filled {} ({} chars)",
            smix_selector::describe_selector(&sel),
            params.text.chars().count()
        ))]))
    }

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
                Ok(CallToolResult::success(vec![Content::text(format!(
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
                Ok(CallToolResult::success(vec![Content::text(format!(
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
        description = "Swipe until an element comes into view, then stop. Use this rather than repeated swipes — it knows when to stop. Not for ocrText — swipe with smix_swipe and check with smix_find between swipes instead. Needs the session smix_launch_app opens (SMIX_UDID env var set)."
    )]
    /// CLI: smix scroll
    async fn smix_scroll(
        &self,
        Parameters(params): Parameters<ScrollParams>,
    ) -> Result<CallToolResult, McpError> {
        let sel = params.target.to_selector()?;
        if chain_of(&sel).is_some() {
            return Err(McpError::invalid_params(
                "a chain means 'try these in order on this screen', and scrolling changes the screen between tries, so the later layers would answer a different question. Probing the whole chain after each swipe is a capability smix does not have yet; it is named here rather than approximated",
                None,
            ));
        }
        if point_of(&sel).is_some() {
            return Err(McpError::invalid_params(
                "a point is already a place on this screen; scrolling to it means nothing — the schema says so and now this does too. Name the \
                 element with id / text / label / role / ocrText, or tap the \
                 point and act on what the tap put on screen.",
                None,
            ));
        }
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
        let app = self.bound_app().await?;
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
    /// CLI: smix sim launch
    async fn smix_launch_app(
        &self,
        Parameters(params): Parameters<BundleParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut app = self.bound_app_mut().await?;
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
    /// CLI: smix sim terminate
    async fn smix_stop_app(
        &self,
        Parameters(params): Parameters<BundleParams>,
    ) -> Result<CallToolResult, McpError> {
        let app = self.bound_app().await?;
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
    /// CLI: smix wait-for
    async fn smix_assert_visible(
        &self,
        Parameters(params): Parameters<SelectorParams>,
    ) -> Result<CallToolResult, McpError> {
        let sel = params.to_selector()?;
        if point_of(&sel).is_some() {
            return Err(McpError::invalid_params(
                "a point names a place, not an element, so there is nothing here to \
                 assert. Only smix_tap takes one. Name the element with id / text / \
                 label / role / ocrText, or tap the point and check what the tap \
                 landed on.",
                None,
            ));
        }
        let app = self.bound_app().await?;
        if let Some(layers) = chain_of(&sel) {
            for (i, layer) in layers.iter().enumerate() {
                if point_of(layer).is_some() {
                    return Err(McpError::invalid_params(
                        format!(
                            "fallback[{i}] is a point, and a point is a place rather \
                             than something that can be seen. Only smix_tap takes one"
                        ),
                        None,
                    ));
                }
            }
            let hit = first_visible_layer(&app, &layers).await?;
            return Ok(CallToolResult::success(vec![Content::text(match hit {
                Some(i) => format!(
                    "visible: fallback[{i}] {}",
                    smix_selector::describe_selector(&layers[i])
                ),
                None => {
                    return Err(McpError::internal_error(
                        format!("all {} fallback layers missed", layers.len()),
                        None,
                    ));
                }
            })]));
        }
        match ocr_text_of(&sel) {
            // Same budget and cadence as the tree path: `App::assert_visible`
            // waits 5 s at the driver's 250 ms poll interval.
            Some(needle) => {
                let timeout = Duration::from_secs(5);
                let start = std::time::Instant::now();
                loop {
                    let hit = app
                        .find_by_text_ocr(needle, ocr_locales_of(&sel))
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
    /// CLI: smix wait-for --absent
    async fn smix_assert_not_visible(
        &self,
        Parameters(params): Parameters<SelectorParams>,
    ) -> Result<CallToolResult, McpError> {
        let sel = params.to_selector()?;
        if point_of(&sel).is_some() {
            return Err(McpError::invalid_params(
                "a point names a place, not an element, so there is nothing here to \
                 assert the absence of. Only smix_tap takes one. Name the element with id / text / \
                 label / role / ocrText, or tap the point and check what the tap \
                 landed on.",
                None,
            ));
        }
        let app = self.bound_app().await?;
        if let Some(layers) = chain_of(&sel) {
            for (i, layer) in layers.iter().enumerate() {
                if point_of(layer).is_some() {
                    return Err(McpError::invalid_params(
                        format!(
                            "fallback[{i}] is a point, and a point is a place rather \
                             than something that can be seen. Only smix_tap takes one"
                        ),
                        None,
                    ));
                }
            }
            let hit = first_visible_layer(&app, &layers).await?;
            return Ok(CallToolResult::success(vec![Content::text(match hit {
                Some(i) => {
                    return Err(McpError::internal_error(
                        format!(
                            "fallback[{i}] {} is visible",
                            smix_selector::describe_selector(&layers[i])
                        ),
                        None,
                    ));
                }
                None => format!("not visible: all {} fallback layers", layers.len()),
            })]));
        }
        match ocr_text_of(&sel) {
            // A tree-routed OcrText never matches, so this assert used to
            // pass vacuously — a false green. Probe OCR once, mirroring the
            // tree path's single non-waiting `find`.
            Some(needle) => {
                let hit = app
                    .find_by_text_ocr(needle, ocr_locales_of(&sel))
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
    /// CLI: smix press-key
    async fn smix_press_key(
        &self,
        Parameters(params): Parameters<PressKeyParams>,
    ) -> Result<CallToolResult, McpError> {
        let k = parse_key_name(&params.key).map_err(|m| McpError::invalid_params(m, None))?;
        let app = self.bound_app().await?;
        app.press_key(k)
            .await
            .map_err(|e| McpError::internal_error(e.to_prompt(), None))?;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "pressed: {}",
            params.key
        ))]))
    }

    #[tool(
        description = "Tap an element and take a frame in the same call, in that order. For UI that does not wait: something that hides itself after a few seconds outlives neither a second tool call nor the turn between them. Name the element with exactly one of id / text / label / role — prefer id. Returns a line saying where the frame came from and how many milliseconds after the tap it landed, then the PNG as base64. A tap that fails returns no frame. Needs the session smix_launch_app opens."
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
        let b64 = base64::engine::general_purpose::STANDARD.encode(&captured.png);
        Ok(CallToolResult::success(vec![
            Content::text(said),
            Content::text(b64),
        ]))
    }

    #[tool(
        description = "Capture a base64-PNG screenshot of the current screen. Requires the SMIX_UDID env var (set it in the MCP server config)."
    )]
    /// CLI: smix sim screenshot
    async fn smix_screenshot(&self) -> Result<CallToolResult, McpError> {
        let app = self.bound_app().await?;
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

    #[tool(description = "Snapshot the runner's runtime state: recent simctl \
            invocations, open app sessions, sim-health, supervisor pid, uptime, \
            and the app-alive / session lifecycle counters. Read-only diagnosis. \
            Needs a device bound (smix_use); does not need an app session.")]
    /// CLI: smix diagnostic dump
    async fn smix_diagnostic_dump(&self) -> Result<CallToolResult, McpError> {
        let app = self.bound_app().await?;
        let client = app.http_runner_client().ok_or_else(|| {
            McpError::internal_error(
                "this session's driver is not backed by an HTTP runner",
                None,
            )
        })?;
        let dump = client
            .diagnostic_dump()
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let json = serde_json::to_string_pretty(&dump).unwrap_or_default();
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        description = "Which device this session is bound to (udid, port), and \
            whether anything is bound yet. Read-only, and the one tool that \
            works with nothing bound — it answers 'what am I driving?' that \
            smix_use sets up. Does not open or close sessions."
    )]
    /// CLI: smix session state
    async fn smix_session_state(&self) -> Result<CallToolResult, McpError> {
        let json = smix_mcp::session_state_report(self.session.current().as_ref());
        Ok(CallToolResult::success(vec![Content::text(json)]))
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
    // Answer `--version` before anything else touches stdio.
    //
    // Without this, asking the server its version made it treat an empty
    // stdin as a request and print a JSON-RPC parse error — on stdout, in
    // a shape whose digits (`-32700`) read as a version number to anything
    // scraping them. The plugin's readiness hook did exactly that and told
    // sessions there was a version mismatch that did not exist. A binary
    // that ships should be able to say what it is.
    let mut args = std::env::args().skip(1);
    if let Some(flag) = args.next()
        && (flag == "--version" || flag == "-V")
    {
        println!("smix-mcp {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

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

    let session = smix_mcp::SessionState::from_env(std::env::var("SMIX_UDID").ok(), port);
    let service = SmixMcpService::new(app, session);
    let server = service.serve(stdio()).await?;
    server.waiting().await?;
    Ok(())
}
