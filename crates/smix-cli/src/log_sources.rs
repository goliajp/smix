//! Signal sources a `smix run` reads beside `--metro-log-url`.

use std::path::PathBuf;

#[derive(clap::Args, Debug, Clone, Default)]
pub struct LogSources {
    /// Read the app's own log on the device as a signal source: the iOS
    /// unified log for the app's subsystem (its bundle id), or Android's
    /// logcat for the app's uid — live, from the start of the run, so
    /// `expect: { signal }` matches a line the app writes after the step
    /// begins. Needs `--device`.
    #[arg(long = "app-log", default_value_t = false)]
    pub app_log: bool,
    /// Tail this file as a signal source too (repeatable), beside the app's
    /// log and `--metro-log-url`: a dev server's log, say. Every source
    /// feeds the same `expect: { signal }`.
    #[arg(long = "log-file")]
    pub log_files: Vec<PathBuf>,
}
