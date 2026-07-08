//! v1.0 Phase C2 — bridge between yaml `takeScreenshot: { annotate }`
//! specs and the [`smix_annotate`] renderer.
//!
//! # Position resolution
//!
//! - [`AnnotationPos::Pixel`] → `smix_annotate::Position::pixel(x, y)`
//! - [`AnnotationPos::Normalized`] → `smix_annotate::Position::normalized(nx, ny)`
//! - [`AnnotationPos::Selector`] → **v1.0: unsupported in yaml**, logged
//!   as a warning and defaults to (0, 0). Rationale: adapter's
//!   `AppLike` trait exposes `find(bool)` but not tree-fetch; wiring
//!   selector → pixel requires the tree accessor which is scheduled
//!   for Phase E authoring tier (adds `find_center` API). Consumers
//!   who need selector-relative annotations today use `assertScreenshot`
//!   baseline with pre-computed pixel coords via `smix authoring
//!   tap-record` (Phase E ships).

use crate::{AnnotationPos, AnnotationSpec};
use smix_annotate::{Annotation, Annotator, Color, Compression, Position};

/// Render annotations onto the PNG bytes. Returns the annotated PNG
/// plus any advisory warnings (unresolved selector positions etc.).
pub fn render_yaml_annotations(
    png_bytes: &[u8],
    specs: &[AnnotationSpec],
) -> Result<(Vec<u8>, Vec<String>), String> {
    let mut warnings = Vec::new();
    let mut annotator = Annotator::new(png_bytes)
        .map_err(|e| format!("decode input PNG: {e}"))?
        .compression(Compression::BALANCED);

    for spec in specs {
        let ann = spec_to_annotation(spec, &mut warnings);
        annotator = annotator.add(ann);
    }
    let out = annotator
        .render()
        .map_err(|e| format!("render annotations: {e}"))?;
    Ok((out, warnings))
}

fn spec_to_annotation(spec: &AnnotationSpec, warnings: &mut Vec<String>) -> Annotation {
    match spec {
        AnnotationSpec::Circle {
            at,
            color,
            radius,
            stroke,
        } => {
            let pos = pos_to_position(at, "circle.at", warnings);
            let col = parse_color(color, Color::RED, "circle.color", warnings);
            Annotation::circle(pos)
                .color(col)
                .radius(*radius)
                .stroke(*stroke)
                .build()
        }
        AnnotationSpec::Line {
            from,
            to,
            color,
            stroke,
        } => {
            let f = pos_to_position(from, "line.from", warnings);
            let t = pos_to_position(to, "line.to", warnings);
            let col = parse_color(color, Color::CYAN, "line.color", warnings);
            Annotation::line(f, t).color(col).stroke(*stroke).build()
        }
        AnnotationSpec::Arrow {
            from,
            to,
            color,
            stroke,
        } => {
            let f = pos_to_position(from, "arrow.from", warnings);
            let t = pos_to_position(to, "arrow.to", warnings);
            let col = parse_color(color, Color::BLUE, "arrow.color", warnings);
            Annotation::arrow(f, t).color(col).stroke(*stroke).build()
        }
        AnnotationSpec::Text {
            at,
            content,
            color,
            size,
        } => {
            let pos = pos_to_position(at, "text.at", warnings);
            let col = parse_color(color, Color::WHITE, "text.color", warnings);
            Annotation::text(pos, content.clone())
                .color(col)
                .size(*size)
                .build()
        }
        AnnotationSpec::Box {
            at,
            width,
            height,
            color,
            stroke,
        } => {
            let pos = pos_to_position(at, "box.at", warnings);
            let col = parse_color(color, Color::YELLOW, "box.color", warnings);
            Annotation::box_(pos, *width, *height)
                .color(col)
                .stroke(*stroke)
                .build()
        }
    }
}

fn pos_to_position(p: &AnnotationPos, field: &str, warnings: &mut Vec<String>) -> Position {
    match p {
        AnnotationPos::Pixel { x, y } => Position::pixel(*x, *y),
        AnnotationPos::Normalized { nx, ny } => Position::normalized(*nx, *ny),
        AnnotationPos::Selector(sel) => {
            warnings.push(format!(
                "{field}: selector-relative positions unsupported in yaml verb (v1.0); \
                 defaulting to (0, 0). Use {{x, y}} or {{nx, ny}} or wait for Phase E \
                 `smix authoring tap-record`. selector={sel:?}"
            ));
            Position::pixel(0, 0)
        }
    }
}

fn parse_color(spec: &str, default: Color, field: &str, warnings: &mut Vec<String>) -> Color {
    match Color::parse(spec) {
        Ok(c) => c,
        Err(e) => {
            warnings.push(format!("{field}: {e}; using default {default:?}"));
            default
        }
    }
}
