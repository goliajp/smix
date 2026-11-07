//! `smix pinch <selector>` — two fingers about an element or a point.

use smix_driver::Platform;
use smix_input::{Pan, Pinch, PinchSpans, RunUp};
use smix_selector::Selector;

use crate::act::{ActError, driver_for, parse_selector};
use crate::platform::RunPlatform;

/// What the command line said, before anything is resolved.
#[derive(clap::Args, Debug)]
pub struct PinchArgs {
    /// The element whose centre the fingers meet at, in `<kind>:<value>`
    /// shorthand, or `point:X%,Y%`.
    pub selector: String,
    /// Zoom factor at the start. Pairs with `--to`.
    #[arg(long)]
    pub from: Option<f64>,
    /// Zoom factor at the end. The larger of the two puts the fingers
    /// 0.6 of the screen's shorter side apart.
    #[arg(long)]
    pub to: Option<f64>,
    /// Finger distance at the start, as a share of the screen's shorter
    /// side. Pairs with `--to-span`; instead of `--from`/`--to`.
    #[arg(long)]
    pub from_span: Option<f64>,
    /// Finger distance at the end, same unit as `--from-span`.
    #[arg(long)]
    pub to_span: Option<f64>,
    /// Length of the pinch, in milliseconds (50..=10000).
    #[arg(long, default_value_t = smix_input::PINCH_DEFAULT_DURATION_MS)]
    pub duration_ms: u64,
    /// Drag both fingers this far after the pinch, before lifting:
    /// `DX,DY` as shares of the screen (`0,-0.2` or `0%,-20%`).
    #[arg(long, allow_hyphen_values = true)]
    pub pan: Option<String>,
    /// Length of the drag, in milliseconds.
    #[arg(long, default_value_t = smix_input::PINCH_DEFAULT_PAN_MS)]
    pub pan_ms: u64,
    /// How the fingers reach the start before the counted pinch:
    /// `spread` (the default) comes down a few points inside the start
    /// and opens to it, which UIKit's recognizers and Android's
    /// ScaleGestureDetector need; `slide` comes down at the start's span
    /// and slides to it, for a recognizer that counts from the touch;
    /// `none` starts at once.
    #[arg(long, default_value = "spread")]
    pub run_up: String,
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

/// The finger distances the flags name: one whole pair.
fn spans(a: &PinchArgs) -> Result<PinchSpans, String> {
    match (a.from, a.to, a.from_span, a.to_span) {
        (Some(from), Some(to), None, None) => Ok(PinchSpans::Scales { from, to }),
        (None, None, Some(from), Some(to)) => Ok(PinchSpans::Spans { from, to }),
        _ => Err(
            "give the zoom as `--from` and `--to` (factors, e.g. 1 and 3), or the finger \
                  distances as `--from-span` and `--to-span` (shares of the screen's shorter \
                  side) — one pair, both ends"
                .into(),
        ),
    }
}

/// `DX,DY` as shares of the screen, either sign; `%` allowed.
fn pan(s: &str) -> Result<(f64, f64), String> {
    let num = |t: &str| {
        let t = t.trim();
        match t.strip_suffix('%') {
            Some(p) => p.trim().parse::<f64>().map(|n| n / 100.0),
            None => t.parse::<f64>(),
        }
        .map_err(|_| format!("--pan {s}: expected `DX,DY`, e.g. `0,-0.2` or `0%,-20%`"))
    };
    let (x, y) = s
        .split_once(',')
        .ok_or_else(|| format!("--pan {s}: expected `DX,DY`, e.g. `0,-0.2` or `0%,-20%`"))?;
    Ok((num(x)?, num(y)?))
}

pub async fn cmd_pinch(args: PinchArgs, port: u16, platform: Platform) -> Result<(), ActError> {
    let selector = parse_selector(&args.selector)
        .map_err(|why| ActError::BadSelector(args.selector.clone(), why))?;
    let bad = |why: String| ActError::BadSelector(args.selector.clone(), why);
    let (from_span, to_span) = spans(&args).and_then(|s| s.resolve()).map_err(bad)?;
    let pan = args
        .pan
        .as_deref()
        .map(|s| pan(s).map(|(dx, dy)| Pan::new(dx, dy, args.pan_ms)))
        .transpose()
        .map_err(bad)?;
    let run_up = RunUp::from_name(&args.run_up).ok_or_else(|| {
        bad(format!(
            "--run-up {}: expected spread, slide or none",
            args.run_up
        ))
    })?;
    let d = driver_for(platform, port);
    let centre = match &selector {
        Selector::Point { nx, ny } => (*nx, *ny),
        _ => {
            let tree = d
                .tree(None)
                .await
                .map_err(|e| ActError::Transport(e.to_prompt()))?;
            smix_host_coord_resolver::resolve_to_norm_point_in_box(&tree, &selector, (0.5, 0.5))
                .map_err(|e| bad(format!("no centre to pinch about: {e:?}")))?
        }
    };
    let pinch = Pinch::new(centre, from_span, to_span)
        .with_duration_ms(args.duration_ms)
        .with_pan(pan)
        .with_run_up(run_up);
    pinch.check().map_err(bad)?;
    d.pinch(&pinch)
        .await
        .map_err(|e| ActError::Transport(e.to_prompt()))?;
    let drag = pan.map_or(String::new(), |p| {
        format!(", then dragged ({:.2}, {:.2})", p.dx, p.dy)
    });
    println!(
        "pinched about ({:.3}, {:.3}): fingers {:.2} → {:.2} of the shorter side over {} ms{drag} — \
         not verified: check the app's zoom with the next step",
        centre.0, centre.1, from_span, to_span, args.duration_ms
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args() -> PinchArgs {
        PinchArgs {
            selector: "point:50%,50%".into(),
            from: Some(1.0),
            to: Some(3.0),
            from_span: None,
            to_span: None,
            duration_ms: 500,
            pan: None,
            pan_ms: 300,
            run_up: "spread".into(),
            port: None,
            device: None,
            platform: None,
        }
    }

    #[test]
    fn one_pair_of_distances() {
        assert_eq!(
            spans(&args()),
            Ok(PinchSpans::Scales { from: 1.0, to: 3.0 })
        );
        let both = PinchArgs {
            from_span: Some(0.2),
            to_span: Some(0.6),
            ..args()
        };
        assert!(spans(&both).unwrap_err().contains("one pair"));
        let half = PinchArgs { to: None, ..args() };
        assert!(spans(&half).is_err());
    }

    #[test]
    fn pan_reads_shares_and_percentages_of_either_sign() {
        assert_eq!(pan("0,-0.2"), Ok((0.0, -0.2)));
        assert_eq!(pan("10%, -20%"), Ok((0.1, -0.2)));
        assert!(pan("up").is_err());
    }
}
