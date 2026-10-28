//! `smix pinch <selector>` — two fingers about an element or a point.

use smix_driver::Platform;
use smix_input::{Pan, Pinch, PinchSpans};
use smix_selector::Selector;

use crate::act::{ActError, driver_for, parse_selector};

/// What the command line said, before anything is resolved.
pub struct PinchArgs {
    pub selector: String,
    pub from: Option<f64>,
    pub to: Option<f64>,
    pub from_span: Option<f64>,
    pub to_span: Option<f64>,
    pub duration_ms: u64,
    pub pan: Option<String>,
    pub pan_ms: u64,
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
        .map(|s| {
            pan(s).map(|(dx, dy)| Pan {
                dx,
                dy,
                ms: args.pan_ms,
            })
        })
        .transpose()
        .map_err(bad)?;
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
    let pinch = Pinch {
        centre,
        from_span,
        to_span,
        duration_ms: args.duration_ms,
        pan,
    };
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
