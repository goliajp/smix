//! Driving a runner, for the Swift and Kotlin SDKs.
//!
//! A thin wrapper over `smix_runner_client`, which is the wire. The SDKs used
//! to each implement the wire themselves, and three of the four drifted into
//! calling routes no runner serves — adding an endpoint meant editing four
//! codebases, so nobody did. One implementation means one edit.
//!
//! This is exported by proc-macro, not declared in `smix.udl`, and that is
//! not a preference. UDL scaffolding never wraps a future in a tokio
//! `Compat` — `async_runtime` does not appear anywhere in `uniffi_bindgen` —
//! while `#[uniffi::export(async_runtime = "tokio")]` does. The client is
//! reqwest, so a UDL-declared async fn here would be polled with no reactor
//! and panic at runtime, with nothing to catch it at compile time.

use std::sync::Arc;

use smix_runner_client::HttpRunnerClient;
use smix_runner_client::{SessionAppLifecycleRequest, SessionOpenRequest};
use smix_input::{KeyName, SwipeDirection};
use tokio_util::sync::CancellationToken;

/// What can go wrong driving the runner.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum DriveError {
    /// The runner could not be reached, or answered with a failure.
    ///
    /// The field is `detail`, not `message`: uniffi's Kotlin bindings make an
    /// error variant's fields into properties, and a `message` property
    /// collides with `Throwable.message` — the generated Kotlin does not
    /// compile.
    #[error("{detail}")]
    Transport {
        /// What the transport reported, verbatim.
        detail: String,
    },
    /// The caller cancelled the call before the runner answered.
    #[error("cancelled")]
    Cancelled,
}

impl From<smix_runner_client::RunnerTransportError> for DriveError {
    fn from(e: smix_runner_client::RunnerTransportError) -> Self {
        DriveError::Transport {
            detail: e.to_string(),
        }
    }
}

/// Cancels a call that is still in flight.
///
/// Explicit, because a foreign `Task.cancel()` does not reach Rust: uniffi
/// 0.29.5 generates `rust_future_cancel` and neither the Swift nor the Kotlin
/// backend ever calls it, and Swift's async bridge suspends on
/// `withUnsafeContinuation` with no cancellation handler. Exporting a method
/// that looks cancellable through the language's own idiom and is not would
/// be a surface claiming something it does not do — the exact defect this
/// crate's neighbours are here to undo.
#[derive(uniffi::Object)]
pub struct CancelToken {
    inner: CancellationToken,
}

#[uniffi::export]
impl CancelToken {
    /// Cancel whatever call was given this token. Idempotent.
    pub fn cancel(&self) {
        self.inner.cancel();
    }

    /// Whether `cancel` has been called.
    pub fn is_cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }
}

/// A runner, over HTTP on localhost.
#[derive(uniffi::Object)]
pub struct SmixDriver {
    client: Arc<HttpRunnerClient>,
}

/// Run `fut`, giving up if `token` is cancelled first.
async fn until_cancelled<T, E>(
    token: Option<Arc<CancelToken>>,
    fut: impl std::future::Future<Output = Result<T, E>>,
) -> Result<T, DriveError>
where
    DriveError: From<E>,
{
    match token {
        None => fut.await.map_err(DriveError::from),
        Some(token) => {
            tokio::select! {
                // Cancellation wins a tie: a caller who has said stop is not
                // interested in an answer that arrived at the same moment.
                biased;
                () = token.inner.cancelled() => Err(DriveError::Cancelled),
                result = fut => result.map_err(DriveError::from),
            }
        }
    }
}

/// Parse a wire enum from the name the SDK sent, using serde's own
/// camelCase contract so there is no second copy of the mapping to drift.
fn parse_wire_enum<T: serde::de::DeserializeOwned>(name: &str, what: &str) -> Result<T, DriveError> {
    serde_json::from_value(serde_json::Value::String(name.to_string())).map_err(|_| {
        DriveError::Transport {
            detail: format!("unknown {what}: {name:?}"),
        }
    })
}

#[uniffi::export(async_runtime = "tokio")]
impl SmixDriver {
    /// Point at a runner on `port` of this machine.
    #[uniffi::constructor]
    #[must_use]
    pub fn new(port: u16) -> Arc<Self> {
        Arc::new(SmixDriver {
            client: Arc::new(HttpRunnerClient::new(port)),
        })
    }

    /// A fresh token, for one call.
    #[must_use]
    pub fn cancel_token(&self) -> Arc<CancelToken> {
        Arc::new(CancelToken {
            inner: CancellationToken::new(),
        })
    }

    /// The accessibility tree, as JSON.
    ///
    /// JSON rather than a typed tree: `A11yNode` is recursive, and the
    /// selector core on this same boundary already takes the tree as JSON.
    pub async fn tree(&self, cancel: Option<Arc<CancelToken>>) -> Result<String, DriveError> {
        let tree = until_cancelled(cancel, self.client.get_tree(None)).await?;
        serde_json::to_string(&tree).map_err(|e| DriveError::Transport {
            detail: format!("could not encode the tree: {e}"),
        })
    }

    /// Open a session bound to `bundle_id`.
    ///
    /// Everything that acts on an app goes through one, because the runner's
    /// session routes require the id — it names the cached application
    /// binding they act on.
    pub async fn open_session(
        &self,
        bundle_id: String,
        cancel: Option<Arc<CancelToken>>,
    ) -> Result<Arc<SmixSession>, DriveError> {
        let req = SessionOpenRequest {
            bundle_id,
            // The runner activates on open when asked. Left off: the caller
            // says when the app should come forward, by launching it.
            activate: false,
        };
        let response = until_cancelled(cancel, self.client.open_session(&req)).await?;
        Ok(Arc::new(SmixSession {
            // The same client, so a session reuses the connection pool the
            // driver already has rather than opening its own.
            client: Arc::clone(&self.client),
            session_id: response.session_id,
        }))
    }

    /// The sessions the runner currently holds open, as JSON. For a caller
    /// reconciling its own handles with the runner's view.
    pub async fn list_sessions(
        &self,
        cancel: Option<Arc<CancelToken>>,
    ) -> Result<String, DriveError> {
        let sessions = until_cancelled(cancel, self.client.list_sessions()).await?;
        serde_json::to_string(&sessions).map_err(|e| DriveError::Transport {
            detail: format!("could not encode the sessions: {e}"),
        })
    }
}

/// An open session. Holds the id the runner's app routes require, so there
/// is no way to ask for one of them without it.
#[derive(uniffi::Object)]
pub struct SmixSession {
    client: Arc<HttpRunnerClient>,
    session_id: String,
}

#[uniffi::export(async_runtime = "tokio")]
impl SmixSession {
    /// The runner's token for this session.
    #[must_use]
    pub fn id(&self) -> String {
        self.session_id.clone()
    }

    /// Launch the session's app.
    pub async fn launch_app(&self, cancel: Option<Arc<CancelToken>>) -> Result<(), DriveError> {
        let req = SessionAppLifecycleRequest {
            session_id: self.session_id.clone(),
            ..Default::default()
        };
        until_cancelled(cancel, self.client.launch_session_app(&req)).await?;
        Ok(())
    }

    /// Terminate the session's app.
    pub async fn terminate_app(&self, cancel: Option<Arc<CancelToken>>) -> Result<(), DriveError> {
        let req = SessionAppLifecycleRequest {
            session_id: self.session_id.clone(),
            ..Default::default()
        };
        until_cancelled(cancel, self.client.terminate_session_app(&req)).await?;
        Ok(())
    }

    /// Tap the element with this accessibility id. Returns whether the
    /// runner found one to tap.
    ///
    /// This is how the SDKs tap: they resolve a selector to an id and pass
    /// it here, so no coordinate crosses the boundary. There is deliberately
    /// no absolute-pixel tap — the runner does not take one.
    pub async fn tap_by_id(
        &self,
        id: String,
        cancel: Option<Arc<CancelToken>>,
    ) -> Result<bool, DriveError> {
        until_cancelled(cancel, self.client.tap_by_id(&id)).await
    }

    /// Tap at a normalized coordinate, both in `0.0..=1.0`. The escape
    /// hatch, for targets with no accessibility semantics to select on.
    pub async fn tap_at_norm_coord(
        &self,
        nx: f64,
        ny: f64,
        cancel: Option<Arc<CancelToken>>,
    ) -> Result<(), DriveError> {
        until_cancelled(cancel, self.client.tap_at_norm_coord(nx, ny)).await
    }

    /// Type into the focused element.
    pub async fn input_text(
        &self,
        text: String,
        cancel: Option<Arc<CancelToken>>,
    ) -> Result<(), DriveError> {
        until_cancelled(cancel, self.client.input_text(&text)).await
    }

    /// Press a hardware-like key. `key` is a name the runner knows —
    /// "return", "delete", "arrowUp"; an unknown one is refused here, before
    /// any request, so the string boundary is not a way to send nonsense on.
    pub async fn press_key(
        &self,
        key: String,
        cancel: Option<Arc<CancelToken>>,
    ) -> Result<(), DriveError> {
        let key: KeyName = parse_wire_enum(&key, "key")?;
        // The runner returns a post-keystroke tree snapshot and timings; the
        // SDK's pressKey is fire-and-return, so that diagnostic payload is
        // dropped here rather than widening the boundary for it.
        until_cancelled(cancel, self.client.press_key(key))
            .await
            .map(|_result| ())
    }

    /// Swipe so the named direction of content comes into view. `direction`
    /// is "up", "down", "left" or "right".
    pub async fn swipe_once(
        &self,
        direction: String,
        cancel: Option<Arc<CancelToken>>,
    ) -> Result<(), DriveError> {
        let direction: SwipeDirection = parse_wire_enum(&direction, "direction")?;
        until_cancelled(cancel, self.client.swipe_once(direction)).await
    }

    /// System alerts and permission dialogs currently on screen, as JSON.
    pub async fn system_popups(
        &self,
        cancel: Option<Arc<CancelToken>>,
    ) -> Result<String, DriveError> {
        let popups = until_cancelled(cancel, self.client.system_popups(None)).await?;
        serde_json::to_string(&popups).map_err(|e| DriveError::Transport {
            detail: format!("could not encode the popups: {e}"),
        })
    }

    /// Relaunch the session's app.
    ///
    /// This is a plain relaunch. Clearing state before relaunch is
    /// launch-fresh orchestration, which lives on the host (simctl/adb) and
    /// not on this device-side boundary.
    pub async fn relaunch_app(&self, cancel: Option<Arc<CancelToken>>) -> Result<(), DriveError> {
        let req = smix_runner_client::SessionRelaunchAppRequest {
            session_id: self.session_id.clone(),
        };
        until_cancelled(cancel, self.client.relaunch_session_app(&req)).await?;
        Ok(())
    }

    /// Renew the session's activation, so the app stays foregrounded across a
    /// long-running flow.
    pub async fn renew_activation(
        &self,
        cancel: Option<Arc<CancelToken>>,
    ) -> Result<(), DriveError> {
        let req = smix_runner_client::SessionRenewActivationRequest {
            session_id: self.session_id.clone(),
        };
        until_cancelled(cancel, self.client.renew_session_activation(&req)).await?;
        Ok(())
    }

    /// Close the session, releasing the runner's cached app binding.
    /// Idempotent — closing an already-closed session is not an error.
    pub async fn close(&self, cancel: Option<Arc<CancelToken>>) -> Result<(), DriveError> {
        let req = smix_runner_client::SessionCloseRequest {
            session_id: self.session_id.clone(),
        };
        until_cancelled(cancel, self.client.close_session(&req)).await?;
        Ok(())
    }
}
