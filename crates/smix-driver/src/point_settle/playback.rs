//! Frames compared for a point touch, with a playing video left out.
//!
//! A whole-screen comparison never reads a screen with a live video on it
//! as still, so every point touch there waited out the full limit (3.4 s,
//! measured by a consumer on a home screen with a live stream across its
//! top half). A screen arriving moves everywhere for a few hundred
//! milliseconds; a video moves in one place for as long as it plays. So a
//! sample that has changed in every comparison for longer than
//! [`PERSIST`] is taken as playing and stops counting — unless it is near
//! the point, where whatever is moving is what the touch would land on.

use std::time::Duration;
use tokio::time::Instant;

use crate::still::{GrayView, QuiescenceParams};

/// How long a sample has to keep changing to be taken as playing.
///
/// Far above a transition: a pushed screen, a sheet or a drawer animates
/// for 0.3–0.5 s on both platforms.
pub(crate) const PERSIST: Duration = Duration::from_secs(1);

/// How far from the point, as a fraction of the screen on each axis,
/// motion always counts. A control is well inside this; a drawer or sheet
/// arriving under the point moves inside it.
const NEAR: (f64, f64) = (0.15, 0.10);

/// Part of the screen, in whole percent of its width and height.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenRegion {
    pub left: u8,
    pub top: u8,
    pub right: u8,
    pub bottom: u8,
}

/// One comparison of two frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Look {
    Still,
    /// Still apart from samples taken as playing, which moved this time
    /// and lie inside this region.
    StillApartFrom(ScreenRegion),
    Moving,
}

#[derive(Clone, Copy, Default)]
struct Sample {
    moving_since: Option<Instant>,
    playing: bool,
}

/// What has moved, sample by sample, over one wait.
pub(crate) struct Watch {
    params: QuiescenceParams,
    point: (f64, f64),
    samples: Vec<Sample>,
}

impl Watch {
    pub(crate) fn new(params: QuiescenceParams, nx: f64, ny: f64) -> Self {
        let grid = params.grid.max(1);
        Self {
            params,
            point: (nx, ny),
            samples: vec![Sample::default(); grid * grid],
        }
    }

    /// Compare two frames taken back to back, the later one at `at`.
    pub(crate) fn compare(&mut self, a: &GrayView, b: &GrayView, at: Instant) -> Look {
        if a.w != b.w || a.h != b.h {
            // a rotation: everything moved, and nothing learnt about where
            // a video is carries over to the new shape
            self.samples.fill(Sample::default());
            return Look::Moving;
        }
        let grid = self.params.grid.max(1);
        let mut counted = 0usize;
        let mut playing: Option<(usize, usize, usize, usize)> = None;
        for gy in 0..grid {
            for gx in 0..grid {
                let (sx, sy) = ((gx * a.w) / grid, (gy * a.h) / grid);
                let moved = a.gray(sx, sy).abs_diff(b.gray(sx, sy)) > self.params.pixel_delta;
                let near = self.near(gx, gy, grid);
                let s = &mut self.samples[gy * grid + gx];
                if !moved {
                    if !s.playing {
                        s.moving_since = None;
                    }
                    continue;
                }
                let since = *s.moving_since.get_or_insert(at);
                if !near && at.duration_since(since) >= PERSIST {
                    s.playing = true;
                }
                if s.playing {
                    playing = Some(match playing {
                        None => (gx, gy, gx, gy),
                        Some((l, t, r, btm)) => (l.min(gx), t.min(gy), r.max(gx), btm.max(gy)),
                    });
                } else {
                    counted += 1;
                }
            }
        }
        if counted > self.params.max_moved_samples {
            return Look::Moving;
        }
        match playing {
            None => Look::Still,
            Some((l, t, r, btm)) => Look::StillApartFrom(ScreenRegion {
                left: percent(l, grid),
                top: percent(t, grid),
                right: percent(r + 1, grid),
                bottom: percent(btm + 1, grid),
            }),
        }
    }

    fn near(&self, gx: usize, gy: usize, grid: usize) -> bool {
        let cx = (gx as f64 + 0.5) / grid as f64;
        let cy = (gy as f64 + 0.5) / grid as f64;
        (cx - self.point.0).abs() <= NEAR.0 && (cy - self.point.1).abs() <= NEAR.1
    }
}

fn percent(cell: usize, grid: usize) -> u8 {
    u8::try_from((cell * 100) / grid).unwrap_or(100)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::still::decode_gray;

    const W: u32 = 96;
    const H: u32 = 96;

    fn encode(mut pixel: impl FnMut(u32, u32) -> u8) -> GrayView {
        let mut raw = Vec::with_capacity((W * H) as usize);
        for y in 0..H {
            for x in 0..W {
                raw.push(pixel(x, y));
            }
        }
        let mut png = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut png, W, H);
            enc.set_color(png::ColorType::Grayscale);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header().unwrap().write_image_data(&raw).unwrap();
        }
        decode_gray(&png).unwrap()
    }

    /// The top half plays: a different picture each frame. The rest is flat.
    fn video_frame(n: u8) -> GrayView {
        encode(|x, y| {
            if y < H / 2 {
                n.wrapping_mul(37).wrapping_add(x as u8)
            } else {
                200
            }
        })
    }

    fn watch(nx: f64, ny: f64) -> Watch {
        Watch::new(QuiescenceParams::default(), nx, ny)
    }

    #[test]
    fn a_video_away_from_the_point_stops_counting_once_it_has_played_past_the_persist() {
        let mut w = watch(0.5, 0.75);
        let t0 = Instant::now();
        let mut looks = Vec::new();
        for i in 1..=8u8 {
            let at = t0 + Duration::from_millis(200) * u32::from(i);
            looks.push(w.compare(&video_frame(i - 1), &video_frame(i), at));
        }
        // under a second of playing it is motion like any other
        assert!(looks[..4].iter().all(|l| *l == Look::Moving), "{looks:?}");
        let Some(Look::StillApartFrom(r)) = looks.last() else {
            panic!("{looks:?}");
        };
        assert_eq!((r.left, r.top, r.right, r.bottom), (0, 0, 100, 50));
    }

    #[test]
    fn a_video_under_the_point_always_counts() {
        let mut w = watch(0.5, 0.25);
        let t0 = Instant::now();
        for i in 1..=12u8 {
            let at = t0 + Duration::from_millis(200) * u32::from(i);
            assert_eq!(
                w.compare(&video_frame(i - 1), &video_frame(i), at),
                Look::Moving
            );
        }
    }

    #[test]
    fn a_transition_shorter_than_the_persist_is_waited_for_and_then_still() {
        // the whole screen slides for 0.4 s, then holds
        let slide = |off: u32| encode(move |x, _| ((x + off) * 2) as u8);
        let mut w = watch(0.5, 0.75);
        let t0 = Instant::now();
        assert_eq!(
            w.compare(&slide(0), &slide(8), t0 + Duration::from_millis(200)),
            Look::Moving
        );
        assert_eq!(
            w.compare(&slide(8), &slide(16), t0 + Duration::from_millis(400)),
            Look::Moving
        );
        assert_eq!(
            w.compare(&slide(16), &slide(16), t0 + Duration::from_millis(600)),
            Look::Still
        );
    }

    #[test]
    fn a_sample_that_pauses_starts_its_persist_again() {
        let mut w = watch(0.5, 0.75);
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        assert_eq!(
            w.compare(&video_frame(0), &video_frame(1), at(200)),
            Look::Moving
        );
        assert_eq!(
            w.compare(&video_frame(1), &video_frame(2), at(800)),
            Look::Moving
        );
        // one still pair before the second is up: not yet playing
        assert_eq!(
            w.compare(&video_frame(2), &video_frame(2), at(1000)),
            Look::Still
        );
        assert_eq!(
            w.compare(&video_frame(2), &video_frame(3), at(1300)),
            Look::Moving
        );
    }

    #[test]
    fn a_change_of_shape_is_motion_and_forgets_what_was_playing() {
        let mut w = watch(0.5, 0.75);
        let t0 = Instant::now();
        for i in 1..=8u8 {
            w.compare(
                &video_frame(i - 1),
                &video_frame(i),
                t0 + Duration::from_millis(200) * u32::from(i),
            );
        }
        let wide = {
            let mut raw = vec![0u8; (H * W * 2) as usize];
            raw.iter_mut().for_each(|p| *p = 9);
            let mut png = Vec::new();
            {
                let mut enc = png::Encoder::new(&mut png, W * 2, H);
                enc.set_color(png::ColorType::Grayscale);
                enc.set_depth(png::BitDepth::Eight);
                enc.write_header().unwrap().write_image_data(&raw).unwrap();
            }
            decode_gray(&png).unwrap()
        };
        assert_eq!(
            w.compare(&video_frame(8), &wide, t0 + Duration::from_secs(2)),
            Look::Moving
        );
        assert!(w.samples.iter().all(|s| !s.playing));
    }
}
