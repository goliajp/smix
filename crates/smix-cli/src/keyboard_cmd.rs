//! `smix sim keyboard <device> [keys…]` — press keys on a simulator's
//! hardware keyboard, or switch Full Keyboard Access.

use smix_simctl::SimctlClient;
use smix_simctl::keyboard::{Chord, press};

/// What the command line said.
#[derive(clap::Args, Debug)]
pub struct KeyboardArgs {
    /// Which simulator, by UDID or registry alias.
    pub device: String,
    /// Keys to press, one after another: `tab shift+tab down space return
    /// escape`, letters, and ctrl/shift/alt/cmd with `+`. Left out, print
    /// whether Full Keyboard Access is on.
    pub keys: Vec<Chord>,
    /// Switch Full Keyboard Access, which is what lets Tab, the arrows and
    /// Space reach controls that are not text fields. On needs a reboot to
    /// take effect; off does not.
    #[arg(long, value_enum, conflicts_with = "keys")]
    pub full_access: Option<OnOff>,
    /// Shut the simulator down and boot it back up after writing
    /// `--full-access`.
    #[arg(long, requires = "full_access")]
    pub reboot: bool,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnOff {
    On,
    Off,
}

/// A runner holds the simulator in UI-test mode, where Full Keyboard
/// Access shows its outline and then takes no further key: measured with
/// the same keys on one boot, runner up and down in turn.
fn refuse_beside_a_runner(udid: &str, what: &str) -> Result<(), crate::CliError> {
    match crate::runner_target::recorded_runner_port(udid) {
        None => Ok(()),
        Some(port) => Err(crate::CliError::Other(format!(
            "{udid} has a runner up on port {port}, and {what}. Stop it first:\n  \
             smix runner down --device {udid}\n\
             `smix sim screenshot` works without one."
        ))),
    }
}

pub async fn run(simctl: &SimctlClient, args: KeyboardArgs) -> Result<(), crate::CliError> {
    let udid = crate::resolve_device(&args.device)?;
    if let Some(to) = args.full_access {
        let on = to == OnOff::On;
        if args.reboot {
            refuse_beside_a_runner(&udid, "a reboot would kill it")?;
        }
        simctl.set_full_keyboard_access(&udid, on).await?;
        let word = if on { "on" } else { "off" };
        if args.reboot {
            simctl.shutdown(&udid).await?;
            simctl.boot(&udid).await?;
            println!("keyboard: {udid} full keyboard access {word} (rebooted)");
        } else if on {
            println!(
                "keyboard: {udid} full keyboard access written on — it takes effect at the next boot (`--reboot` does it now)"
            );
        } else {
            println!("keyboard: {udid} full keyboard access {word}");
        }
        return Ok(());
    }
    let on = simctl.full_keyboard_access(&udid).await?;
    let state = if on { "on" } else { "off" };
    if args.keys.is_empty() {
        println!("keyboard: {udid} full keyboard access {state}");
        return Ok(());
    }
    refuse_beside_a_runner(
        &udid,
        "while one is, Full Keyboard Access takes the first key and ignores the rest",
    )?;
    press(&udid, &args.keys)
        .await
        .map_err(crate::CliError::Other)?;
    let keys: Vec<&str> = args.keys.iter().map(Chord::as_str).collect();
    println!(
        "keyboard: {udid} pressed {} (full keyboard access {state})",
        keys.join(" ")
    );
    Ok(())
}
