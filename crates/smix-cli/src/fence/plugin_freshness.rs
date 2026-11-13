//! Whether the Claude Code plugin is older than this binary.
//!
//! The plugin carries the hooks that keep a session's commands inside
//! the fence. One older than the CLI enforces older rules, and the only
//! notice was a line at session start that is easy to miss, so `smix
//! doctor` fails on it and every refusal repeats it.

use std::path::{Path, PathBuf};

const CLI: &str = env!("CARGO_PKG_VERSION");

/// `a` is an older release than `b`. Versions that are not three numbers
/// compare as not older: this says "update", and a guess is no reason to.
pub(crate) fn older(a: &str, b: &str) -> bool {
    fn parts(v: &str) -> Option<[u64; 3]> {
        let mut it = v.trim().split('.').map(|p| p.parse::<u64>().ok());
        let parts = [it.next()??, it.next()??, it.next()??];
        it.next().is_none().then_some(parts)
    }
    matches!((parts(a), parts(b)), (Some(a), Some(b)) if a < b)
}

fn manifest_version(plugin_root: &Path) -> Option<String> {
    let text = std::fs::read_to_string(plugin_root.join(".claude-plugin/plugin.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    json.get("version")?.as_str().map(str::to_string)
}

fn update_line(plugin: &str) -> String {
    format!(
        "the smix plugin here is {plugin} and smix is {CLI}; its hooks enforce \
         {plugin}'s rules. Update it: `claude plugin marketplace update goliajp && \
         claude plugin update smix@goliajp`, then restart Claude Code."
    )
}

/// For a refusal printed from inside the plugin's hook, where Claude Code
/// sets `CLAUDE_PLUGIN_ROOT`.
pub(crate) fn hook_plugin_staleness() -> Option<String> {
    let root = std::env::var_os("CLAUDE_PLUGIN_ROOT")?;
    let plugin = manifest_version(Path::new(&root))?;
    older(&plugin, CLI).then(|| update_line(&plugin))
}

/// Claude Code's config directory for this shell.
fn config_dir() -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".claude")))
}

/// What `smix doctor` says about the installed plugin: `Ok` with a line
/// when it is current or absent, `Err` with the update line when older.
pub(crate) fn doctor_verdict() -> Result<String, String> {
    let Some(dir) = config_dir() else {
        return Ok("plugin: no Claude Code config directory".into());
    };
    let path = dir.join("plugins/installed_plugins.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(format!(
            "plugin: none installed ({} absent)",
            path.display()
        ));
    };
    let versions = installed_versions(&text);
    if versions.is_empty() {
        return Ok("plugin: smix plugin not installed".into());
    }
    match versions.iter().find(|v| older(v, CLI)) {
        Some(old) => Err(update_line(old)),
        None => Ok(format!("plugin: {} (smix {CLI})", versions.join(", "))),
    }
}

/// Versions of every installed `smix@<marketplace>` entry.
pub(crate) fn installed_versions(installed_plugins_json: &str) -> Vec<String> {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(installed_plugins_json) else {
        return Vec::new();
    };
    let Some(plugins) = json.get("plugins").and_then(|p| p.as_object()) else {
        return Vec::new();
    };
    plugins
        .iter()
        .filter(|(name, _)| name.starts_with("smix@"))
        .flat_map(|(_, rows)| rows.as_array().into_iter().flatten())
        .filter_map(|row| row.get("version")?.as_str().map(str::to_string))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_as_numbers() {
        assert!(older("25.1.0", "26.0.0"));
        assert!(older("9.9.9", "10.0.0"), "not as text");
        assert!(!older("26.1.0", "26.1.0"));
        assert!(!older("26.2.0", "26.1.0"));
        assert!(!older("unknown", "26.1.0"), "a guess is no reason to nag");
    }

    #[test]
    fn the_installed_smix_plugin_is_read_from_claude_codes_record() {
        let text = r#"{"version":2,"plugins":{
            "smix@goliajp":[{"scope":"user","version":"25.1.0"}],
            "other@x":[{"version":"1.0.0"}]}}"#;
        assert_eq!(installed_versions(text), vec!["25.1.0".to_string()]);
        assert!(installed_versions("{}").is_empty());
    }
}
