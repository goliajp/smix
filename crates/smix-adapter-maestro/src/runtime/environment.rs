//! `setEnvironment`: settings a run changes and puts back when it ends.
//!
//! A device-level setting is read first, written to the device's ledger
//! with that value, then set and read back. The ledger is what a killed run
//! leaves to `smix cleanup`; a run that ends puts the values back itself.
//! An iOS simulator keeps no language or time zone of its own: those are
//! the app's, given to it by the launches after this step.

use std::collections::BTreeMap;

use smix_lease::environment::{self, EnvChange, commands};

use super::{Adapter, RunError, RunStepReport, refused::refused};
use crate::AppLike;

/// What this run changed, and what its iOS launches are to carry.
#[derive(Debug, Default)]
pub(super) struct EnvState {
    /// The device driven, when the caller said.
    device: Option<String>,
    /// Device settings changed, each with its value before; oldest first.
    changed: Vec<EnvChange>,
    /// `locale` / `timeZone` for iOS launches.
    launch: BTreeMap<String, String>,
    /// Whether this run has launched the app yet.
    launched: bool,
}

/// Run a blocking settings command off the async runtime.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| format!("settings command panicked: {e}"))?
}

fn ledger() -> Result<smix_lease::store::LeaseDir, String> {
    smix_lease::store::LeaseDir::machine()
        .ok_or_else(|| "no machine ledger directory to keep the values from before in".into())
}

impl<A: AppLike + ?Sized> Adapter<'_, A> {
    /// Name the device this run drives, so `setEnvironment` can change it.
    #[must_use]
    pub fn with_device_id(mut self, device_id: impl Into<String>) -> Self {
        self.environment.device = Some(device_id.into());
        self
    }

    pub(super) async fn set_environment(
        &mut self,
        settings: &[(String, String)],
    ) -> Result<RunStepReport, RunError> {
        let Some(device) = self.environment.device.clone() else {
            return Err(
                refused("setEnvironment: this run was not told which device it drives").into(),
            );
        };
        let fail = |e: String| -> RunError { refused(format!("setEnvironment: {e}")).into() };
        for (setting, value) in settings {
            let wanted = commands::device_value(&device, setting, value).or_else(|e| {
                if commands::is_ios(&device) && matches!(setting.as_str(), "locale" | "timeZone") {
                    Ok(value.clone())
                } else {
                    Err(e)
                }
            });
            let wanted = wanted.map_err(fail)?;
            if commands::is_ios(&device) && matches!(setting.as_str(), "locale" | "timeZone") {
                if self.environment.launched {
                    return Err(fail(format!(
                        "an iOS simulator has no {setting} of its own; it is given to the app \
                         when it launches, and this run has launched it already — put \
                         setEnvironment before launchApp"
                    )));
                }
                self.environment.launch.insert(setting.clone(), wanted);
                continue;
            }
            let app = (setting == "locale")
                .then(|| self.last_bundle.clone())
                .flatten();
            let (d, s, a) = (device.clone(), setting.clone(), app.clone());
            let before = blocking(move || {
                environment::current(&d, &s, a.as_deref(), &smix_adb::std_command)
            })
            .await
            .map_err(fail)?;
            let change = EnvChange {
                setting: setting.clone(),
                before,
                app: app.clone(),
            };
            environment::record(&ledger().map_err(fail)?, &device, change.clone())
                .map_err(|e| fail(e.to_string()))?;
            self.environment.changed = environment::with_change(&self.environment.changed, change);
            let (d, s) = (device.clone(), setting.clone());
            blocking(move || {
                environment::put(&d, &s, &wanted, app.as_deref(), &smix_adb::std_command)
            })
            .await
            .map_err(fail)?;
        }
        Ok(RunStepReport::Ok)
    }

    /// The variables an iOS launch carries: `TZ` for a time zone.
    pub(super) fn launch_environment(&mut self) -> Vec<(String, String)> {
        self.environment.launched = true;
        self.environment
            .launch
            .get("timeZone")
            .map(|tz| vec![("TZ".to_string(), tz.clone())])
            .unwrap_or_default()
    }

    /// `arguments`, with the language an iOS launch is to carry.
    pub(super) fn launch_arguments(&self, arguments: &[String]) -> Vec<String> {
        let mut out = arguments.to_vec();
        if let Some(tag) = self.environment.launch.get("locale") {
            out.extend([
                "-AppleLanguages".into(),
                format!("({tag})"),
                "-AppleLocale".into(),
                tag.replace('-', "_"),
            ]);
        }
        out
    }

    /// Put back what this run changed, newest first, and drop it from the
    /// ledger; one line for each setting that could not be put back.
    pub(super) async fn restore_environment(&mut self) -> Vec<String> {
        let (Some(device), false) = (
            self.environment.device.clone(),
            self.environment.changed.is_empty(),
        ) else {
            return Vec::new();
        };
        let changes = std::mem::take(&mut self.environment.changed);
        let d = device.clone();
        let failed = tokio::task::spawn_blocking(move || {
            environment::restore(&d, &changes, &smix_adb::std_command)
        })
        .await
        .unwrap_or_else(|e| vec![format!("putting the settings back panicked: {e}")]);
        if failed.is_empty()
            && let Err(e) =
                ledger().and_then(|l| environment::forget(&l, &device).map_err(|e| e.to_string()))
        {
            return vec![format!(
                "settings put back, but the ledger still lists them: {e}"
            )];
        }
        failed
    }
}
