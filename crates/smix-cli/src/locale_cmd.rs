//! `smix sim locale <device> <lang>` — a simulator's language.

use smix_simctl::SimctlClient;

pub async fn run(
    simctl: &SimctlClient,
    device: &str,
    lang: &str,
    reboot: bool,
) -> Result<(), crate::CliError> {
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
