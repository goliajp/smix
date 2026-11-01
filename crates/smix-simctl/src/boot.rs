//! Booting a simulator, with its PosterBoard store checked first.

use std::time::Duration;

use tokio::process::Command;

use crate::SubprocessRecord;
use crate::poster_store;
use crate::subprocess_ring;
use crate::{DeviceControlError, SimctlClient, simctl_run};

impl SimctlClient {
    /// `xcrun simctl boot <udid>` — fire-and-forget boot request.
    pub async fn boot(&self, udid: &str) -> Result<(), DeviceControlError> {
        self.mend_poster_store(udid).await?;
        simctl_run(&["boot", udid]).await?;
        Ok(())
    }

    /// Boot the device and return once it has finished booting, within
    /// `timeout`. A device that is already booted and ready returns at
    /// once (about 0.1 s).
    ///
    /// "Booted" in the device list is not "finished": it is listed so
    /// while SpringBoard is still coming up — measured 8.9 s before
    /// `simctl bootstatus` said it was done — and an XCUITest runner
    /// started in that gap cannot launch its app and exits with status
    /// 65 having run no test. So the wait is `bootstatus`, which also
    /// covers a device someone else booted a moment ago.
    pub async fn boot_and_wait(
        &self,
        udid: &str,
        timeout: Duration,
    ) -> Result<(), DeviceControlError> {
        self.mend_poster_store(udid).await?;
        let started = std::time::Instant::now();
        let mut cmd = Command::new("xcrun");
        cmd.args(["simctl", "bootstatus", udid, "-b"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        let output = match tokio::time::timeout(timeout, cmd.output()).await {
            Ok(output) => output?,
            Err(_) => {
                return Err(DeviceControlError::Timeout {
                    subcommand: format!("bootstatus {udid} -b"),
                    ms: timeout.as_millis() as u64,
                });
            }
        };
        let argv: Vec<String> = ["xcrun", "simctl", "bootstatus", udid, "-b"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let wall_ms = started.elapsed().as_millis() as u64;
        subprocess_ring::record(SubprocessRecord {
            argv: argv.clone(),
            exit_code: output.status.code(),
            wall_ms,
            stderr_head: stderr.chars().take(256).collect(),
            timestamp: std::time::SystemTime::now(),
        });
        if !output.status.success() {
            return Err(DeviceControlError::NonZeroExit {
                subcommand: "bootstatus".to_string(),
                argv,
                code: output.status.code().unwrap_or(-1),
                stderr,
                wall_ms,
            });
        }
        Ok(())
    }

    /// Before a boot: a PosterBoard store PosterBoard cannot load is
    /// removed while the simulator is off, so this boot writes a new one
    /// instead of starting a crash loop. One found on a simulator already
    /// running is only reported — it may be in someone's hands in
    /// Simulator.app, which no lease shows — with the command that mends it.
    async fn mend_poster_store(&self, udid: &str) -> Result<(), DeviceControlError> {
        let Some(data) = poster_store::data_dir(udid) else {
            return Ok(());
        };
        let broken = poster_store::broken_posters(&data);
        let Some(first) = broken.first() else {
            return Ok(());
        };
        let what = if broken.len() == 1 {
            first.to_string()
        } else {
            format!("{first}, and {} more", broken.len() - 1)
        };
        let state = self
            .list_devices()
            .await?
            .into_iter()
            .find(|d| d.udid.eq_ignore_ascii_case(udid))
            .map(|d| d.state);
        if state.as_deref() != Some("Shutdown") {
            eprintln!(
                "smix: warning: PosterBoard on {udid} cannot load its store ({what}); \
                 it crashes at every start once it reads it. \
                 `smix sim repair {udid}` shuts the simulator down and rebuilds the store"
            );
            return Ok(());
        }
        poster_store::rebuild(&data).map_err(|source| DeviceControlError::PosterStore {
            udid: udid.to_string(),
            broken: what.clone(),
            source,
        })?;
        eprintln!(
            "smix: PosterBoard on {udid} could not load its store ({what}); removed it so \
             PosterBoard writes a new one at this boot — the wallpaper and clock face go \
             back to their defaults"
        );
        Ok(())
    }
}
