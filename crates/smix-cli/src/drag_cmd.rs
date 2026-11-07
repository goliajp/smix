//! `smix long-press <target>` and `smix drag <from> <to>` — a held touch,
//! in place or moved.

use std::time::Duration;

use smix_driver::Platform;
use smix_input::Drag;
use smix_selector::{Selector, describe_selector};

use crate::act::{ActError, driver_for, parse_selector};
use crate::platform::RunPlatform;

/// What `smix long-press` was given.
#[derive(clap::Args, Debug)]
pub struct LongPressArgs {
    /// What to press, in `<kind>:<value>` shorthand, or `point:X%,Y%` for a
    /// place with nothing nameable, such as an empty spot on a home screen.
    pub target: String,
    /// How long the finger stays down, in milliseconds.
    #[arg(long, default_value_t = 500)]
    pub duration_ms: u64,
    /// Runner port; unset: SMIX_RUNNER_PORT, the runner smix brought up on --device, its registered runnerPort, 22087 (iOS) / 28080 (Android).
    #[arg(long, alias = "runner-port")]
    pub port: Option<u16>,
    /// Device UDID, or an alias in this machine's registry.
    #[arg(long)]
    pub device: Option<String>,
    /// Which platform the device is, for a device this machine has not
    /// registered.
    #[arg(long, value_enum)]
    pub platform: Option<RunPlatform>,
}

/// What `smix drag` was given.
#[derive(clap::Args, Debug)]
pub struct DragArgs {
    /// Where the finger goes down: `<kind>:<value>` or `point:X%,Y%`.
    pub from: String,
    /// Where it lifts, same forms.
    pub to: String,
    /// How long the finger stays still before moving, in milliseconds.
    /// Long enough for an icon or widget to come loose; on an iOS home
    /// screen a hold past about 700 ms opens the icon's menu first, and the
    /// move then picks a menu item instead of carrying the icon.
    #[arg(long, default_value_t = smix_input::DRAG_DEFAULT_HOLD_MS)]
    pub hold_ms: u64,
    /// How long the move takes, in milliseconds.
    #[arg(long, default_value_t = smix_input::DRAG_DEFAULT_DURATION_MS)]
    pub duration_ms: u64,
    /// How long the finger rests at the end before lifting, in
    /// milliseconds, so a drop target such as "Remove" sees it.
    #[arg(long, default_value_t = smix_input::DRAG_DEFAULT_RELEASE_MS)]
    pub release_ms: u64,
    /// Runner port; unset: SMIX_RUNNER_PORT, the runner smix brought up on --device, its registered runnerPort, 22087 (iOS) / 28080 (Android).
    #[arg(long, alias = "runner-port")]
    pub port: Option<u16>,
    /// Device UDID, or an alias in this machine's registry.
    #[arg(long)]
    pub device: Option<String>,
    /// Which platform the device is, for a device this machine has not
    /// registered.
    #[arg(long, value_enum)]
    pub platform: Option<RunPlatform>,
}

fn target(s: &str) -> Result<Selector, ActError> {
    parse_selector(s).map_err(|e| ActError::BadSelector(s.to_string(), e))
}

fn failed(e: smix_error::ExpectationFailure) -> ActError {
    ActError::Transport(e.to_prompt())
}

pub async fn cmd_long_press(
    args: LongPressArgs,
    port: u16,
    platform: Platform,
) -> Result<(), ActError> {
    let sel = target(&args.target)?;
    let d = driver_for(platform, port);
    match sel {
        // A place, not an element: located, then pressed there.
        Selector::Point { .. } | Selector::OcrText { .. } => {
            let (nx, ny) = smix_driver::locate(d.as_ref(), &sel)
                .await
                .map_err(failed)?;
            d.long_press_at_norm_coord(nx, ny, args.duration_ms)
                .await
                .map_err(failed)?;
        }
        _ => {
            d.long_press(&sel, Duration::from_millis(args.duration_ms), None)
                .await
                .map_err(failed)?;
        }
    }
    println!(
        "long-pressed {} for {} ms",
        describe_selector(&sel),
        args.duration_ms
    );
    Ok(())
}

pub async fn cmd_drag(args: DragArgs, port: u16, platform: Platform) -> Result<(), ActError> {
    let (from, to) = (target(&args.from)?, target(&args.to)?);
    let d = driver_for(platform, port);
    let drag = Drag::new(
        smix_driver::locate(d.as_ref(), &from)
            .await
            .map_err(failed)?,
        smix_driver::locate(d.as_ref(), &to).await.map_err(failed)?,
    )
    .with_hold_ms(args.hold_ms)
    .with_duration_ms(args.duration_ms)
    .with_release_ms(args.release_ms);
    drag.check()
        .map_err(|why| ActError::BadSelector(format!("{} → {}", args.from, args.to), why))?;
    d.drag(&drag).await.map_err(failed)?;
    println!(
        "dragged ({:.3}, {:.3}) → ({:.3}, {:.3}): held {} ms, moved {} ms, rested {} ms — \
         not verified: check where the item landed with the next step",
        drag.from.0,
        drag.from.1,
        drag.to.0,
        drag.to.1,
        drag.hold_ms,
        drag.duration_ms,
        drag.release_ms
    );
    Ok(())
}
