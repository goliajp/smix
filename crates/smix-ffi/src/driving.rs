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
use tokio_util::sync::CancellationToken;

/// What can go wrong driving the runner.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum DriveError {
    /// The runner could not be reached, or answered with a failure.
    #[error("{message}")]
    Transport {
        /// What the transport reported, verbatim.
        message: String,
    },
    /// The caller cancelled the call before the runner answered.
    #[error("cancelled")]
    Cancelled,
}

impl From<smix_runner_client::RunnerTransportError> for DriveError {
    fn from(e: smix_runner_client::RunnerTransportError) -> Self {
        DriveError::Transport {
            message: e.to_string(),
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
            message: format!("could not encode the tree: {e}"),
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
}
