//! A launch whose process is started with environment variables.

use smix_error::{ExpectationFailure, FailureCode, FailureInit};

use crate::{App, Permission, PermissionAction};

/// Typed shape of maestro yaml `launchApp:` mapping. Adapter assembles this
/// from yaml fields; SDK consumes it in [`App::launch_app_with_options`].
/// Covers maestro `launchApp.permissions / arguments / stopApp`.
#[derive(Clone, Debug, PartialEq)]
pub struct LaunchAppOptions {
    pub bundle_id: String,
    pub clear_state: bool,
    pub clear_keychain: bool,
    /// Process-level argv passed via `simctl launch -- <args>`.
    pub arguments: Vec<String>,
    /// Permission directives applied in declaration order BEFORE launch.
    ///
    /// The cross-platform [`Permission`], not the iOS spelling. It was
    /// the iOS one until 10.2, which made this whole path — yaml key to
    /// backend — unable to carry a permission iOS has no word for:
    /// `storage` is implemented in the Android backend and a flow could
    /// not name it.
    pub permissions: Vec<(Permission, PermissionAction)>,
    /// App bundle path for clear_state / clear_keychain wipe — mirrors the
    /// `launch_fresh::app_path` parameter; usually populated by the
    /// adapter from `SMIX_APP_PATH_<NORMALIZED_BUNDLE>` env.
    pub app_path: Option<String>,
    /// Variables the app's process is started with (`TZ` for a flow's
    /// time zone). iOS simulators only, where it is a launch's to say;
    /// any other device refuses a non-empty list.
    pub environment: Vec<(String, String)>,
    /// Android launch activity override.
    ///
    /// `None` means "ask the device": the Android runner resolves the
    /// package's launcher activity through the package manager. Set
    /// only when an app has more than one entry point and the flow
    /// wants a specific one. iOS ignores it — a bundle id already
    /// names what to launch.
    pub launch_activity: Option<String>,
}

impl App {
    /// Start the app again with `opts.environment` set: the launch before
    /// this one cleared or started it the ordinary way, and only
    /// `simctl launch` can hand a process its variables.
    pub(crate) async fn relaunch_with_environment(
        &self,
        udid: &str,
        opts: &LaunchAppOptions,
    ) -> Result<(), ExpectationFailure> {
        if self.device.platform() != smix_driver::Platform::Ios || udid.starts_with("emulator-") {
            return Err(ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::DriverError),
                message: "launchApp: starting an app with environment variables is an iOS \
                          simulator's; this device sets those settings itself"
                    .into(),
                ..Default::default()
            }));
        }
        let simctl = smix_simctl::SimctlClient::new();
        // the launch just before started it; one that did not leaves nothing to stop
        let _ = simctl.terminate(udid, &opts.bundle_id).await;
        let env: Vec<(&str, &str)> = opts
            .environment
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        simctl
            .launch_with_args_and_env(udid, &opts.bundle_id, &opts.arguments, &env)
            .await
            .map(|_| ())
            .map_err(crate::simctl_to_failure)
    }
}
