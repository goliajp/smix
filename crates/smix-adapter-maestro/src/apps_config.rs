//! `smix-apps.yaml` resolver.
//!
//! Maps a yaml `app:` logical key (e.g., `demoApp`) to a platform-
//! specific bundle id / package + activity. Enables cross-platform yaml
//! flows: the same yaml runs on iOS + Android, only the smix-apps.yaml
//! config differs per project.
//!
//! ## Config schema
//!
//! ```yaml
//! apps:
//!   demoApp:
//!     ios:
//!       bundleId: com.example.app
//!     android:
//!       package: com.example.app
//!       activity: .MainActivity
//! ```
//!
//! Either platform stanza is optional — a yaml flow referencing a
//! logical key that has no entry for the current platform surfaces a
//! [`ResolveError::UnsupportedPlatform`] (caller decides how to render).

use serde::{Deserialize, Serialize};
use smix_driver::Platform;
use std::collections::HashMap;
use thiserror::Error;

/// Top-level config shape — `apps:` map of logical-key → platform table.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppsConfig {
    /// Logical-key → platform stanzas. Backing store is a [`HashMap`] for
    /// O(1) resolve.
    #[serde(default)]
    pub apps: HashMap<String, AppEntry>,
}

/// Per-app platform stanzas.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppEntry {
    /// iOS bundle id (matches `appId:` literal semantics).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ios: Option<IosApp>,
    /// Android package + activity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub android: Option<AndroidApp>,
}

/// iOS-specific app coords.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IosApp {
    /// Bundle identifier (e.g. `com.example.app`).
    pub bundle_id: String,
}

/// Android-specific app coords.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidApp {
    /// Package name (e.g. `com.example.app`).
    pub package: String,
    /// Activity name (e.g. `.MainActivity` or fully-qualified
    /// `com.example.app.MainActivity`). Defaults to `.MainActivity` if
    /// omitted in yaml.
    #[serde(default = "default_activity")]
    pub activity: String,
}

fn default_activity() -> String {
    ".MainActivity".to_string()
}

/// Resolved platform-specific app identifier returned by
/// [`AppsConfig::resolve`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolvedApp {
    /// iOS bundle id — pass directly to `App::launch(bundle_id)`.
    Ios {
        /// Bundle identifier.
        bundle_id: String,
    },
    /// Android package + activity — `App::launch(package)` works for
    /// `DeviceControl::launch` paths that default to `.MainActivity`.
    Android {
        /// Package name.
        package: String,
        /// Activity name (relative `.MainActivity` or fully-qualified).
        activity: String,
    },
}

impl ResolvedApp {
    /// Return the primary identifier (iOS bundle id or Android package).
    /// Suitable for `App::launch(<bundle_id>)`.
    #[must_use]
    pub fn primary_id(&self) -> &str {
        match self {
            Self::Ios { bundle_id } => bundle_id,
            Self::Android { package, .. } => package,
        }
    }
}

/// Resolver errors.
#[derive(Debug, Error)]
pub enum ResolveError {
    /// Logical key not in config.
    #[error("smix-apps.yaml: no entry for app `{0}`")]
    UnknownApp(String),
    /// Config entry exists but lacks a stanza for the current platform.
    #[error("smix-apps.yaml: app `{app}` has no `{platform:?}` stanza")]
    UnsupportedPlatform {
        /// Logical app key that was missing a stanza.
        app: String,
        /// Current platform that has no stanza.
        platform: Platform,
    },
    /// Yaml parse failed.
    #[error("smix-apps.yaml parse: {0}")]
    Parse(#[from] serde_norway::Error),
    /// I/O failure reading the config file.
    #[error("smix-apps.yaml io: {0}")]
    Io(String),
}

impl AppsConfig {
    /// Parse a `smix-apps.yaml` from a yaml string.
    pub fn from_yaml(yaml: &str) -> Result<Self, ResolveError> {
        let cfg: Self = serde_norway::from_str(yaml)?;
        Ok(cfg)
    }

    /// Parse from a file path.
    pub fn from_path(path: impl AsRef<std::path::Path>) -> Result<Self, ResolveError> {
        let yaml = std::fs::read_to_string(path.as_ref())
            .map_err(|e| ResolveError::Io(format!("{}: {}", path.as_ref().display(), e)))?;
        Self::from_yaml(&yaml)
    }

    /// Resolve a logical key → platform-specific [`ResolvedApp`].
    pub fn resolve(&self, logical: &str, platform: Platform) -> Result<ResolvedApp, ResolveError> {
        let entry = self
            .apps
            .get(logical)
            .ok_or_else(|| ResolveError::UnknownApp(logical.to_string()))?;
        match platform {
            Platform::Ios => entry
                .ios
                .as_ref()
                .map(|ios| ResolvedApp::Ios {
                    bundle_id: ios.bundle_id.clone(),
                })
                .ok_or_else(|| ResolveError::UnsupportedPlatform {
                    app: logical.to_string(),
                    platform,
                }),
            Platform::Android => entry
                .android
                .as_ref()
                .map(|a| ResolvedApp::Android {
                    package: a.package.clone(),
                    activity: a.activity.clone(),
                })
                .ok_or_else(|| ResolveError::UnsupportedPlatform {
                    app: logical.to_string(),
                    platform,
                }),
        }
    }
}

/// Resolve `flow.app` (logical key) into `flow.app_id` literal in
/// place. No-op when the flow already has `app_id` set or no `app`
/// logical key. Recursively patches embedded `Step::LaunchApp { app_id }`
/// inheritors that were left blank by the parser.
pub fn resolve_app_into_flow(
    flow: &mut crate::Flow,
    apps: &AppsConfig,
    platform: Platform,
) -> Result<(), ResolveError> {
    if !flow.app_id.is_empty() {
        // Legacy literal `appId:` — nothing to resolve.
        return Ok(());
    }
    let Some(logical) = flow.app.clone() else {
        return Ok(()); // no logical key — caller dispatches as-is
    };
    let resolved = apps.resolve(&logical, platform)?;
    let primary = resolved.primary_id().to_string();
    flow.app_id = primary.clone();
    // The activity used to stop here: read from the yaml, defaulted,
    // and never looked at again, so `activity: .SomethingElse` was
    // accepted and ignored.
    if let ResolvedApp::Android { activity, .. } = &resolved {
        flow.launch_activity = Some(activity.clone());
    }
    // Patch inherited LaunchApp steps (those with empty app_id) so the
    // runtime dispatch path stays oblivious to the logical resolve.
    for step in &mut flow.steps {
        if let crate::Step::LaunchApp { app_id: sid, .. } = step
            && sid.is_empty()
        {
            *sid = primary.clone();
        }
    }
    Ok(())
}

// ----------------------------- tests -----------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
apps:
  demoApp:
    ios:
      bundleId: com.example.app
    android:
      package: com.example.app
      activity: .DemoActivity
  simpleApp:
    ios:
      bundleId: com.example.simple
    android:
      package: com.example.simple
"#;

    #[test]
    fn parses_sample_config() {
        let cfg = AppsConfig::from_yaml(SAMPLE).expect("parse ok");
        assert!(cfg.apps.contains_key("demoApp"));
        assert!(cfg.apps.contains_key("simpleApp"));
    }

    #[test]
    fn resolve_ios_returns_bundle_id() {
        let cfg = AppsConfig::from_yaml(SAMPLE).unwrap();
        let r = cfg.resolve("demoApp", Platform::Ios).unwrap();
        match r {
            ResolvedApp::Ios { bundle_id } => {
                assert_eq!(bundle_id, "com.example.app");
            }
            _ => panic!("expected Ios"),
        }
    }

    #[test]
    fn resolve_android_returns_package_and_activity() {
        let cfg = AppsConfig::from_yaml(SAMPLE).unwrap();
        let r = cfg.resolve("demoApp", Platform::Android).unwrap();
        match r {
            ResolvedApp::Android { package, activity } => {
                assert_eq!(package, "com.example.app");
                assert_eq!(activity, ".DemoActivity");
            }
            _ => panic!("expected Android"),
        }
    }

    #[test]
    fn resolve_default_activity_when_omitted() {
        let cfg = AppsConfig::from_yaml(SAMPLE).unwrap();
        let r = cfg.resolve("simpleApp", Platform::Android).unwrap();
        match r {
            ResolvedApp::Android { activity, .. } => {
                assert_eq!(activity, ".MainActivity");
            }
            _ => panic!("expected Android"),
        }
    }

    #[test]
    fn resolve_unknown_app_errors() {
        let cfg = AppsConfig::from_yaml(SAMPLE).unwrap();
        let err = cfg.resolve("doesNotExist", Platform::Ios).unwrap_err();
        assert!(matches!(err, ResolveError::UnknownApp(_)));
    }

    #[test]
    fn resolve_unsupported_platform_errors() {
        // App only in ios, lookup as android
        let yaml = r#"
apps:
  iosOnlyApp:
    ios:
      bundleId: com.ios.only
"#;
        let cfg = AppsConfig::from_yaml(yaml).unwrap();
        let err = cfg.resolve("iosOnlyApp", Platform::Android).unwrap_err();
        assert!(matches!(err, ResolveError::UnsupportedPlatform { .. }));
    }

    #[test]
    fn resolve_app_into_flow_patches_logical_yaml() {
        let cfg = AppsConfig::from_yaml(SAMPLE).unwrap();
        let mut flow = crate::Flow {
            app_id: String::new(),
            app: Some("demoApp".to_string()),
            launch_activity: None,
            steps: vec![],
        };
        resolve_app_into_flow(&mut flow, &cfg, Platform::Ios).unwrap();
        assert_eq!(flow.app_id, "com.example.app");
    }

    #[test]
    fn resolve_app_into_flow_noop_when_legacy_app_id_set() {
        let cfg = AppsConfig::from_yaml(SAMPLE).unwrap();
        let mut flow = crate::Flow {
            app_id: "explicit.bundle.id".to_string(),
            app: Some("demoApp".to_string()),
            launch_activity: None,
            steps: vec![],
        };
        resolve_app_into_flow(&mut flow, &cfg, Platform::Ios).unwrap();
        // Legacy literal takes priority — flow.app_id unchanged.
        assert_eq!(flow.app_id, "explicit.bundle.id");
    }
}
