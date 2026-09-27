//! What an Android failure says beyond the failure itself.

use smix_error::{ExpectationFailure, FailureCode, FailureInit};

use crate::android::AndroidDriver;

impl AndroidDriver {
    /// What the reader that just failed cannot see, or nothing when it has
    /// nothing to admit.
    ///
    /// Asked only here, on a path that has already failed, so it costs a
    /// request nobody pays for twice. A Compose dialog's controls arrive in
    /// the accessibility tree as anonymous views: a flow looking for one by
    /// id gets "not found", which is true and useless — the fix is a line
    /// in a build file, and a reader who does not already suspect that will
    /// never guess it from a timeout.
    pub(crate) async fn reader_caveat(&self) -> String {
        let Some(app) = self.runner().target_bundle_id().map(str::to_string) else {
            return String::new();
        };
        match self.runner().probe_status(&app).await {
            Ok(s) if s.present => String::new(),
            Ok(s) => format!(
                "\n  the tree came from the accessibility reader, which sees a \
                 Compose dialog's contents as unnamed views. {} — adding \
                 `debugImplementation(\"jp.golia.smix:smix-probe\")` to its debug \
                 build lets smix read the semantics tree instead.",
                s.why.unwrap_or_else(|| format!("{app} has no smix probe")),
            ),
            // The probe route itself not answering says nothing about the
            // app, and guessing here would send a reader to edit a build
            // file over a runner that is simply older.
            Err(_) => String::new(),
        }
    }
}

/// `dispatch:` overrides are an iOS-runner mechanism.
///
/// The guide says this "errors with an explicit unsupported message";
/// what it actually said was "not implemented by the Kotlin runner",
/// which reads as a missing feature someone should wait for. There is
/// nothing to wait for: Android's default tap already IS native event
/// synthesis, which is what the override buys on iOS. The fix is to
/// drop the key, so the error says that.
pub(crate) fn dispatch_unsupported_err() -> ExpectationFailure {
    ExpectationFailure::new(FailureInit {
        code: Some(FailureCode::DriverError),
        message: "tapOn `dispatch:` is an iOS-runner mechanism and has no \
                  meaning on Android"
            .to_string(),
        hint: Some(
            "remove `dispatch:` from this step — Android taps already use \
             native event synthesis, which is what the override selects on iOS"
                .to_string(),
        ),
        ..Default::default()
    })
}
