//! `smix-maestro` — legacy adapter binary.
//!
//! Backward-compat shim that exposes the lib's [`run_flow`] under the
//! historical `smix-maestro test ...` CLI. Kept around so existing CI
//! scripts (those that shell out to `maestro test` and aliased to
//! `smix-maestro test`) keep working without code change.
//!
//! New code should use `smix run` (in the user-facing `smix` binary)
//! instead. That command calls the same [`run_flow`] entry directly
//! — no proc spawn, no surface duplication.

use clap::{Parser, Subcommand, ValueEnum};
use smix_adapter_maestro::{FlowArgs, FlowPlatform, run_flow};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[command(
    name = "smix-maestro",
    about = "flow-execution adapter — use `smix run` instead",
    version,
    long_about = "\
smix-maestro is the flow-execution adapter that backs the user-facing \
`smix run` subcommand. It exists as a separate binary for historical \
reasons (a Maestro-format yaml compatibility shim, useful for CI scripts \
that already shell out to `maestro test`). New code should use `smix run` \
instead — it is the canonical user-facing surface for executing flow yaml \
on smix.

User-facing equivalent:
  smix-maestro test --udid <UDID> flow.yaml
  ⇨ smix run --device <DEVICE> flow.yaml
"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) cmd: Cmd,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Cmd {
    /// Run a flow YAML file (legacy alias for `smix run`).
    Test {
        /// Path to the flow yaml file.
        flow: PathBuf,
        /// Device UDID or registry alias.
        #[arg(long, env = "SMIX_UDID")]
        udid: Option<String>,
        /// Bundle id of the app under test. Absent = the flow's own
        /// `appId:` header.
        #[arg(long, env = "SMIX_BUNDLE_ID")]
        bundle_id: Option<String>,
        /// Runner HTTP port.
        #[arg(long, env = "SMIX_RUNNER_PORT", default_value = "22087")]
        runner_port: u16,
        /// Skip the initial foreground call.
        #[arg(long, default_value_t = false)]
        no_launch: bool,
        /// Target platform.
        #[arg(long, value_enum, env = "SMIX_PLATFORM", default_value_t = CliPlatform::Ios)]
        platform: CliPlatform,
        /// Path to smix-apps.yaml cross-platform resolver.
        #[arg(long, env = "SMIX_APPS_CONFIG")]
        apps_config: Option<PathBuf>,
    },
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CliPlatform {
    Ios,
    Android,
}

impl CliPlatform {
    fn to_flow(self) -> FlowPlatform {
        match self {
            Self::Ios => FlowPlatform::Ios,
            Self::Android => FlowPlatform::Android,
        }
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Test {
            flow,
            udid,
            bundle_id,
            runner_port,
            no_launch,
            platform,
            apps_config,
        } => {
            run_flow(FlowArgs {
                // The standalone adapter binary has no registry to read
                // a device kind from; it keeps its historical
                // simulator-only reach. The CLI is the entry that knows.
                physical_ios: false,
                flow,
                udid,
                bundle_id,
                runner_port,
                animations: false,
                no_launch,
                platform: platform.to_flow(),
                apps_config,
                // The legacy smix-maestro binary uses defaults for
                // these fields; `smix run` in smix-cli exposes them.
                env_vars: Vec::new(),
                debug_output: None,
                verbose: false,
                format: smix_adapter_maestro::OutputFormat::Human,
                auto_activate: false,
                metro_log_url: None,
                await_signal: None,
                gate_signal: None,
                gate_signal_timeout_ms: 60_000,
                expect_log_clean: false,
                fixture_registry: None,
                force_key_events: false,
                no_fail_annotate: false,
                // The legacy binary injects no config; each switch keeps
                // its own `SMIX_*` env fallback.
                auto_ocr_fallback: None,
                ai_assertions: None,
                assert_screenshot_no_autorecord: None,
                launch_fresh_force_reinstall: None,
            })
            .await
        }
    }
}
