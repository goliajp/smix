//! `smix sim locale <device> <lang>` — a simulator's language.

use smix_simctl::SimctlClient;

/// What the command line said.
#[derive(clap::Args, Debug)]
pub struct LocaleArgs {
    /// Which device, by UDID or registry alias.
    pub device: String,
    /// BCP-47 tag (e.g. `en`, `en-US`, `ja`, `zh-Hans`).
    pub lang: String,
    /// Shut the sim down and boot it back up after writing the
    /// locale, so the change is visible to apps launched next.
    #[arg(long)]
    pub reboot: bool,
}

pub async fn run(simctl: &SimctlClient, args: LocaleArgs) -> Result<(), crate::CliError> {
    let (device, lang, reboot) = (args.device.as_str(), args.lang.as_str(), args.reboot);
    let udid = crate::resolve_device(device)?;
    // Read current locale first — no-op if already desired.
    let current = simctl.current_locale(&udid).await.ok().flatten();
    if current.as_deref() == Some(lang) {
        println!("locale already: {lang}");
        return Ok(());
    }
    simctl.set_locale(&udid, lang).await?;
    if reboot {
        println!("locale: written {lang} — rebooting sim to apply");
        simctl.shutdown(&udid).await?;
        simctl.boot(&udid).await?;
        println!("locale: {lang} enforced (sim rebooted)");
    } else {
        println!(
            "locale: written {lang}\n\
         note: running apps cache locale at process-start — \
         restart the target app, or re-run with `--reboot` to \
         cycle the sim so subsequent launches see the new locale."
        );
    }
    Ok(())
}
