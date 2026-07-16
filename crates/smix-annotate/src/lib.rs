//! smix-annotate — screenshot annotation.
//!
//! Compose circle / arrow / text / box / line primitives onto a PNG,
//! then emit compressed output.
//!
//! # Quick tour
//!
//! ```rust,ignore
//! use smix_annotate::{Annotator, Annotation, Color, Position, Compression};
//!
//! let png = std::fs::read("input.png")?;
//! let out = Annotator::new(&png)?
//!     .add(Annotation::circle(Position::pixel(100, 100))
//!         .color(Color::RED)
//!         .radius(40))
//!     .compression(Compression::BALANCED)
//!     .render()?;
//! std::fs::write("output.png", out)?;
//! ```
//!
//! # Scope
//!
//! - 5 primitives: circle, arrow, text, box, line
//! - Position resolvers: absolute pixel + normalized (0..1)
//!   (selector-relative via a11y tree — wire lives in
//!   `smix-adapter-maestro` runtime, not in this crate; caller
//!   resolves selector → pixel before adding annotation)
//! - Named + hex + rgba color palette
//! - Compression 3 presets: fast, balanced, aggressive
//! - Text rendering via `ab_glyph`; bundled Inter + Noto Sans SC by
//!   default, overridable by the caller via `.font(bytes)`

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use image::{DynamicImage, Rgba, RgbaImage};
use imageproc::drawing;
use thiserror::Error;

pub mod color;
pub mod font;
pub use color::Color;

#[derive(Debug, Error)]
pub enum AnnotateError {
    #[error("decode input PNG: {0}")]
    Decode(#[from] image::ImageError),
    #[error("compress output PNG: {0}")]
    Compress(String),
    #[error("font load: {0}")]
    Font(String),
    /// Retired: the bundled fonts (Inter + Noto SC subset) cover text
    /// rendering out of the box, and consumers can still override with
    /// `.font(bytes)`. Retained for wire compat; no current code path
    /// produces it.
    #[error("text annotation without font — call `.font(bytes)` before `.render()`")]
    MissingFont,
}

/// Position of an annotation on the image.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Position {
    /// Absolute pixel coordinates, top-left origin.
    Pixel { x: i32, y: i32 },
    /// Normalized 0..1 in each dimension. Resolved against the image
    /// size at [`Annotator::render`] time.
    Normalized { nx: f32, ny: f32 },
}

impl Position {
    pub fn pixel(x: i32, y: i32) -> Self {
        Position::Pixel { x, y }
    }

    pub fn normalized(nx: f32, ny: f32) -> Self {
        Position::Normalized { nx, ny }
    }

    fn resolve(self, w: u32, h: u32) -> (i32, i32) {
        match self {
            Position::Pixel { x, y } => (x, y),
            Position::Normalized { nx, ny } => ((nx * w as f32) as i32, (ny * h as f32) as i32),
        }
    }
}

/// One drawing primitive.
#[derive(Clone, Debug)]
pub enum Annotation {
    Circle {
        at: Position,
        color: Color,
        radius: i32,
        stroke: i32,
    },
    Arrow {
        from: Position,
        to: Position,
        color: Color,
        stroke: i32,
        head_filled: bool,
    },
    Text {
        at: Position,
        content: String,
        color: Color,
        size: f32,
    },
    Box {
        at: Position,
        width: i32,
        height: i32,
        color: Color,
        stroke: i32,
    },
    Line {
        from: Position,
        to: Position,
        color: Color,
        stroke: i32,
    },
}

impl Annotation {
    pub fn circle(at: Position) -> CircleBuilder {
        CircleBuilder {
            at,
            color: Color::RED,
            radius: 30,
            stroke: 3,
        }
    }

    pub fn arrow(from: Position, to: Position) -> ArrowBuilder {
        ArrowBuilder {
            from,
            to,
            color: Color::BLUE,
            stroke: 4,
            head_filled: true,
        }
    }

    pub fn text(at: Position, content: impl Into<String>) -> TextBuilder {
        TextBuilder {
            at,
            content: content.into(),
            color: Color::WHITE,
            size: 24.0,
        }
    }

    pub fn box_(at: Position, width: i32, height: i32) -> BoxBuilder {
        BoxBuilder {
            at,
            width,
            height,
            color: Color::YELLOW,
            stroke: 2,
        }
    }

    pub fn line(from: Position, to: Position) -> LineBuilder {
        LineBuilder {
            from,
            to,
            color: Color::CYAN,
            stroke: 2,
        }
    }
}

pub struct CircleBuilder {
    at: Position,
    color: Color,
    radius: i32,
    stroke: i32,
}

impl CircleBuilder {
    pub fn color(mut self, c: Color) -> Self {
        self.color = c;
        self
    }
    pub fn radius(mut self, r: i32) -> Self {
        self.radius = r;
        self
    }
    pub fn stroke(mut self, s: i32) -> Self {
        self.stroke = s;
        self
    }
    pub fn build(self) -> Annotation {
        Annotation::Circle {
            at: self.at,
            color: self.color,
            radius: self.radius,
            stroke: self.stroke,
        }
    }
}

impl From<CircleBuilder> for Annotation {
    fn from(b: CircleBuilder) -> Self {
        b.build()
    }
}

pub struct ArrowBuilder {
    from: Position,
    to: Position,
    color: Color,
    stroke: i32,
    head_filled: bool,
}

impl ArrowBuilder {
    pub fn color(mut self, c: Color) -> Self {
        self.color = c;
        self
    }
    pub fn stroke(mut self, s: i32) -> Self {
        self.stroke = s;
        self
    }
    pub fn head_open(mut self) -> Self {
        self.head_filled = false;
        self
    }
    pub fn build(self) -> Annotation {
        Annotation::Arrow {
            from: self.from,
            to: self.to,
            color: self.color,
            stroke: self.stroke,
            head_filled: self.head_filled,
        }
    }
}

impl From<ArrowBuilder> for Annotation {
    fn from(b: ArrowBuilder) -> Self {
        b.build()
    }
}

pub struct TextBuilder {
    at: Position,
    content: String,
    color: Color,
    size: f32,
}

impl TextBuilder {
    pub fn color(mut self, c: Color) -> Self {
        self.color = c;
        self
    }
    pub fn size(mut self, s: f32) -> Self {
        self.size = s;
        self
    }
    pub fn build(self) -> Annotation {
        Annotation::Text {
            at: self.at,
            content: self.content,
            color: self.color,
            size: self.size,
        }
    }
}

impl From<TextBuilder> for Annotation {
    fn from(b: TextBuilder) -> Self {
        b.build()
    }
}

pub struct BoxBuilder {
    at: Position,
    width: i32,
    height: i32,
    color: Color,
    stroke: i32,
}

impl BoxBuilder {
    pub fn color(mut self, c: Color) -> Self {
        self.color = c;
        self
    }
    pub fn stroke(mut self, s: i32) -> Self {
        self.stroke = s;
        self
    }
    pub fn build(self) -> Annotation {
        Annotation::Box {
            at: self.at,
            width: self.width,
            height: self.height,
            color: self.color,
            stroke: self.stroke,
        }
    }
}

impl From<BoxBuilder> for Annotation {
    fn from(b: BoxBuilder) -> Self {
        b.build()
    }
}

pub struct LineBuilder {
    from: Position,
    to: Position,
    color: Color,
    stroke: i32,
}

impl LineBuilder {
    pub fn color(mut self, c: Color) -> Self {
        self.color = c;
        self
    }
    pub fn stroke(mut self, s: i32) -> Self {
        self.stroke = s;
        self
    }
    pub fn build(self) -> Annotation {
        Annotation::Line {
            from: self.from,
            to: self.to,
            color: self.color,
            stroke: self.stroke,
        }
    }
}

impl From<LineBuilder> for Annotation {
    fn from(b: LineBuilder) -> Self {
        b.build()
    }
}

/// PNG output compression preset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compression {
    /// No oxipng pass; small file overhead but ~5× faster to write.
    Fast,
    /// Balanced default (oxipng optimization 2).
    Balanced,
    /// Maximum shrink (oxipng optimization 6 + zopfli).
    Aggressive,
}

impl Compression {
    pub const FAST: Self = Compression::Fast;
    pub const BALANCED: Self = Compression::Balanced;
    pub const AGGRESSIVE: Self = Compression::Aggressive;
}

#[allow(clippy::derivable_impls)]
impl Default for Compression {
    fn default() -> Self {
        Compression::Balanced
    }
}

/// Annotator builder.
pub struct Annotator {
    image: RgbaImage,
    annotations: Vec<Annotation>,
    font_bytes: Option<Vec<u8>>,
    compression: Compression,
}

impl Annotator {
    /// Decode `png_bytes` and prepare for annotation.
    pub fn new(png_bytes: &[u8]) -> Result<Self, AnnotateError> {
        let dyn_img = image::load_from_memory(png_bytes)?;
        let image = dyn_img.to_rgba8();
        Ok(Self {
            image,
            annotations: Vec::new(),
            font_bytes: None,
            compression: Compression::default(),
        })
    }

    /// Add an annotation. Accepts any builder or a pre-built
    /// [`Annotation`].
    #[allow(clippy::should_implement_trait)]
    pub fn add(mut self, a: impl Into<Annotation>) -> Self {
        self.annotations.push(a.into());
        self
    }

    /// Provide a TTF font for text annotations. Optional (unused
    /// annotations without text primitives).
    pub fn font(mut self, bytes: Vec<u8>) -> Self {
        self.font_bytes = Some(bytes);
        self
    }

    pub fn compression(mut self, c: Compression) -> Self {
        self.compression = c;
        self
    }

    /// Render all annotations and return the PNG-encoded (+ optionally
    /// compressed) output bytes.
    pub fn render(mut self) -> Result<Vec<u8>, AnnotateError> {
        let (w, h) = (self.image.width(), self.image.height());
        // If the consumer supplied .font(bytes), use it
        // for the whole render (override). Otherwise text primitive
        // picks bundled Inter or Noto SC subset per codepoint.
        let override_font: Option<FontRef> = self
            .font_bytes
            .as_ref()
            .map(|b| FontRef::try_from_slice(b).map_err(|e| AnnotateError::Font(e.to_string())))
            .transpose()?;
        for ann in std::mem::take(&mut self.annotations) {
            match ann {
                Annotation::Circle {
                    at,
                    color,
                    radius,
                    stroke,
                } => {
                    let (cx, cy) = at.resolve(w, h);
                    let rgba = Rgba(color.to_rgba());
                    for i in 0..stroke {
                        drawing::draw_hollow_circle_mut(
                            &mut self.image,
                            (cx, cy),
                            (radius - i).max(1),
                            rgba,
                        );
                    }
                }
                Annotation::Arrow {
                    from,
                    to,
                    color,
                    stroke,
                    head_filled,
                } => {
                    let (fx, fy) = from.resolve(w, h);
                    let (tx, ty) = to.resolve(w, h);
                    let rgba = Rgba(color.to_rgba());
                    // Main shaft (thick line = repeat draws with offset).
                    for i in -(stroke / 2)..=stroke / 2 {
                        drawing::draw_line_segment_mut(
                            &mut self.image,
                            (fx as f32 + i as f32, fy as f32),
                            (tx as f32 + i as f32, ty as f32),
                            rgba,
                        );
                    }
                    // Arrow head — two short segments at ±30° from the shaft.
                    let dx = (tx - fx) as f32;
                    let dy = (ty - fy) as f32;
                    let len = (dx * dx + dy * dy).sqrt().max(1.0);
                    let ux = dx / len;
                    let uy = dy / len;
                    let head_len = 15.0_f32.min(len / 3.0);
                    // Rotate ±150° from tip.
                    let angle = 30.0_f32.to_radians();
                    let (sin, cos) = (angle.sin(), angle.cos());
                    let hx1 = tx as f32 - head_len * (ux * cos - uy * sin);
                    let hy1 = ty as f32 - head_len * (uy * cos + ux * sin);
                    let hx2 = tx as f32 - head_len * (ux * cos + uy * sin);
                    let hy2 = ty as f32 - head_len * (uy * cos - ux * sin);
                    drawing::draw_line_segment_mut(
                        &mut self.image,
                        (tx as f32, ty as f32),
                        (hx1, hy1),
                        rgba,
                    );
                    drawing::draw_line_segment_mut(
                        &mut self.image,
                        (tx as f32, ty as f32),
                        (hx2, hy2),
                        rgba,
                    );
                    if head_filled {
                        // Simple filled head — draw a tiny triangle
                        // (three lines connecting the three head points).
                        drawing::draw_line_segment_mut(
                            &mut self.image,
                            (hx1, hy1),
                            (hx2, hy2),
                            rgba,
                        );
                    }
                }
                Annotation::Text {
                    at,
                    content,
                    color,
                    size,
                } => {
                    let (x, y) = at.resolve(w, h);
                    let scale = PxScale::from(size);
                    let rgba = Rgba(color.to_rgba());
                    // Per-codepoint font routing when no
                    // override supplied. Load each bundled font once,
                    // then pick per char.
                    let inter_font = FontRef::try_from_slice(font::INTER_REGULAR)
                        .map_err(|e| AnnotateError::Font(e.to_string()))?;
                    let cjk_font = FontRef::try_from_slice(font::NOTO_SANS_SC_SUBSET)
                        .map_err(|e| AnnotateError::Font(e.to_string()))?;
                    // Compute ascent from Inter (baseline is uniform
                    // across bundled fonts at same scale).
                    let ascent = inter_font.as_scaled(scale).ascent();
                    let mut caret_x = x as f32;
                    let baseline_y = y as f32 + ascent;
                    for ch in content.chars() {
                        // Pick font: override wins; otherwise per-codepoint.
                        let use_font: &FontRef = if let Some(f) = override_font.as_ref() {
                            f
                        } else if font::pick_font_for_codepoint(ch as u32)
                            == font::NOTO_SANS_SC_SUBSET
                        {
                            &cjk_font
                        } else {
                            &inter_font
                        };
                        let scaled = use_font.as_scaled(scale);
                        let glyph = scaled.scaled_glyph(ch);
                        let h_adv = scaled.h_advance(glyph.id);
                        if let Some(outlined) = use_font.outline_glyph(glyph) {
                            let bounds = outlined.px_bounds();
                            outlined.draw(|gx, gy, alpha| {
                                let px = caret_x + bounds.min.x + gx as f32;
                                let py = baseline_y + bounds.min.y + gy as f32;
                                if px >= 0.0 && py >= 0.0 && (px as u32) < w && (py as u32) < h {
                                    let blended = blend_pixel(
                                        self.image.get_pixel(px as u32, py as u32).0,
                                        rgba.0,
                                        alpha,
                                    );
                                    self.image.put_pixel(px as u32, py as u32, Rgba(blended));
                                }
                            });
                        }
                        caret_x += h_adv;
                    }
                }
                Annotation::Box {
                    at,
                    width,
                    height,
                    color,
                    stroke,
                } => {
                    let (x, y) = at.resolve(w, h);
                    let rgba = Rgba(color.to_rgba());
                    for i in 0..stroke {
                        let rect = imageproc::rect::Rect::at(x + i, y + i).of_size(
                            (width - 2 * i).max(1) as u32,
                            (height - 2 * i).max(1) as u32,
                        );
                        drawing::draw_hollow_rect_mut(&mut self.image, rect, rgba);
                    }
                }
                Annotation::Line {
                    from,
                    to,
                    color,
                    stroke,
                } => {
                    let (fx, fy) = from.resolve(w, h);
                    let (tx, ty) = to.resolve(w, h);
                    let rgba = Rgba(color.to_rgba());
                    for i in -(stroke / 2)..=stroke / 2 {
                        drawing::draw_line_segment_mut(
                            &mut self.image,
                            (fx as f32, fy as f32 + i as f32),
                            (tx as f32, ty as f32 + i as f32),
                            rgba,
                        );
                    }
                }
            }
        }
        // Encode.
        let mut raw = Vec::new();
        let dyn_img = DynamicImage::ImageRgba8(self.image);
        dyn_img
            .write_to(&mut std::io::Cursor::new(&mut raw), image::ImageFormat::Png)
            .map_err(AnnotateError::Decode)?;
        // Compression pass.
        match self.compression {
            Compression::Fast => Ok(raw),
            Compression::Balanced => oxipng::optimize_from_memory(
                &raw,
                &oxipng::Options {
                    optimize_alpha: false,
                    ..oxipng::Options::from_preset(2)
                },
            )
            .map_err(|e| AnnotateError::Compress(e.to_string())),
            Compression::Aggressive => oxipng::optimize_from_memory(
                &raw,
                &oxipng::Options {
                    optimize_alpha: false,
                    ..oxipng::Options::from_preset(6)
                },
            )
            .map_err(|e| AnnotateError::Compress(e.to_string())),
        }
    }
}

fn blend_pixel(bg: [u8; 4], fg: [u8; 4], alpha: f32) -> [u8; 4] {
    let a = (fg[3] as f32 / 255.0) * alpha;
    let inv_a = 1.0 - a;
    [
        (bg[0] as f32 * inv_a + fg[0] as f32 * a) as u8,
        (bg[1] as f32 * inv_a + fg[1] as f32 * a) as u8,
        (bg[2] as f32 * inv_a + fg[2] as f32 * a) as u8,
        (bg[3] as f32).max(fg[3] as f32) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank_png() -> Vec<u8> {
        let img: RgbaImage = image::ImageBuffer::from_pixel(100, 100, Rgba([0, 0, 0, 255]));
        let mut out = Vec::new();
        DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    #[test]
    fn circle_pixels_written() {
        let png = blank_png();
        let out = Annotator::new(&png)
            .unwrap()
            .add(
                Annotation::circle(Position::pixel(50, 50))
                    .color(Color::RED)
                    .radius(20),
            )
            .compression(Compression::Fast)
            .render()
            .unwrap();
        let img = image::load_from_memory(&out).unwrap().to_rgba8();
        // The circle passes through (50+20, 50) = (70, 50). Check that
        // pixel is red-ish (dominant R channel).
        let px = img.get_pixel(70, 50);
        assert!(px[0] > 100, "expected red-dominant, got {px:?}");
    }

    #[test]
    fn arrow_pixels_written() {
        let png = blank_png();
        let out = Annotator::new(&png)
            .unwrap()
            .add(Annotation::arrow(
                Position::pixel(10, 10),
                Position::pixel(90, 90),
            ))
            .compression(Compression::Fast)
            .render()
            .unwrap();
        let img = image::load_from_memory(&out).unwrap().to_rgba8();
        // Midpoint (50, 50) should be blue-dominant.
        let px = img.get_pixel(50, 50);
        assert!(px[2] > 100, "expected blue-dominant midpoint, got {px:?}");
    }

    #[test]
    fn box_pixels_written() {
        let png = blank_png();
        let out = Annotator::new(&png)
            .unwrap()
            .add(Annotation::box_(Position::pixel(20, 20), 60, 60).color(Color::YELLOW))
            .compression(Compression::Fast)
            .render()
            .unwrap();
        let img = image::load_from_memory(&out).unwrap().to_rgba8();
        // Top edge of the box: (25, 20) should be yellow (R + G high).
        let px = img.get_pixel(25, 20);
        assert!(
            px[0] > 100 && px[1] > 100,
            "expected yellow edge, got {px:?}"
        );
    }

    #[test]
    fn line_pixels_written() {
        let png = blank_png();
        let out = Annotator::new(&png)
            .unwrap()
            .add(Annotation::line(
                Position::pixel(0, 50),
                Position::pixel(99, 50),
            ))
            .compression(Compression::Fast)
            .render()
            .unwrap();
        let img = image::load_from_memory(&out).unwrap().to_rgba8();
        // Middle of the line: (50, 50) should be cyan (G + B high).
        let px = img.get_pixel(50, 50);
        assert!(
            px[1] > 100 && px[2] > 100,
            "expected cyan midpoint, got {px:?}"
        );
    }

    #[test]
    fn text_without_explicit_font_uses_bundled_inter() {
        // The bundled Inter font renders text without any .font()
        // call, so MissingFont is unreachable here.
        let png = blank_png();
        let out = Annotator::new(&png)
            .unwrap()
            .add(
                Annotation::text(Position::pixel(10, 30), "hello")
                    .color(Color::WHITE)
                    .size(20.0),
            )
            .compression(Compression::Fast)
            .render()
            .expect("bundled font renders ASCII text");
        // Verify some pixel got written (text at (10-30, 20-50)).
        let img = image::load_from_memory(&out).unwrap().to_rgba8();
        let mut has_white = false;
        for x in 10..50u32 {
            for y in 20..50u32 {
                let px = img.get_pixel(x, y);
                if px[0] > 128 && px[1] > 128 && px[2] > 128 {
                    has_white = true;
                }
            }
        }
        assert!(has_white, "bundled font should render visible pixels");
    }

    #[test]
    fn cjk_char_renders_via_bundled_noto_sc() {
        // U+767B (in the Noto SC subset).
        let png = blank_png();
        let out = Annotator::new(&png)
            .unwrap()
            .add(
                Annotation::text(Position::pixel(10, 30), "登录")
                    .color(Color::WHITE)
                    .size(20.0),
            )
            .compression(Compression::Fast)
            .render()
            .expect("bundled Noto SC subset renders CJK");
        let img = image::load_from_memory(&out).unwrap().to_rgba8();
        let mut has_white = false;
        for x in 10..80u32 {
            for y in 20..55u32 {
                let px = img.get_pixel(x, y);
                if px[0] > 128 && px[1] > 128 && px[2] > 128 {
                    has_white = true;
                }
            }
        }
        assert!(has_white, "bundled Noto SC should render CJK pixels");
    }

    #[test]
    fn compression_presets_all_encode() {
        for preset in [
            Compression::Fast,
            Compression::Balanced,
            Compression::Aggressive,
        ] {
            let png = blank_png();
            let out = Annotator::new(&png)
                .unwrap()
                .add(
                    Annotation::circle(Position::pixel(50, 50))
                        .color(Color::RED)
                        .radius(20),
                )
                .compression(preset)
                .render()
                .unwrap();
            assert!(!out.is_empty(), "preset {preset:?} produced empty output");
            // Verify it's valid PNG.
            let _ = image::load_from_memory(&out).unwrap();
        }
    }

    #[test]
    fn normalized_position_resolves() {
        let (x, y) = Position::normalized(0.5, 0.5).resolve(100, 100);
        assert_eq!((x, y), (50, 50));
    }

    #[test]
    fn compression_default_is_balanced() {
        assert_eq!(Compression::default(), Compression::Balanced);
    }
}
