//! The `smix annotate` mini-DSL: one annotation per spec.

/// Parse one annotation spec from the mini-DSL:
///   kind ',' key:value (',' key:value)*
pub(crate) fn parse_annotation_spec(spec: &str) -> Result<smix_annotate::Annotation, String> {
    use smix_annotate::{Annotation, Color, Position};
    let parts: Vec<&str> = spec.split(',').collect();
    let kind = parts
        .first()
        .ok_or_else(|| "empty spec".to_string())?
        .trim();
    let mut kv = std::collections::BTreeMap::new();
    for part in &parts[1..] {
        let (k, v) = part
            .split_once(':')
            .ok_or_else(|| format!("expected key:value, got `{part}`"))?;
        kv.insert(k.trim(), v.trim());
    }
    let get_color = |default: Color| -> Result<Color, String> {
        Ok(match kv.get("color") {
            Some(s) => Color::parse(s).map_err(|e| e.to_string())?,
            None => default,
        })
    };
    let get_pos = |key_prefix: &str, default_key: &str| -> Result<Position, String> {
        let key = if kv.contains_key(key_prefix) {
            key_prefix
        } else {
            default_key
        };
        let v = kv.get(key).ok_or_else(|| format!("missing `{key}`"))?;
        // v is either "X,Y" absolute — but comma already split. Use
        // `at:X_Y` (underscore or pipe separator) instead of `x=X;y=Y`.
        let parts: Vec<&str> = v.split(['_', '|']).collect();
        if parts.len() != 2 {
            return Err(format!(
                "position `{v}` — expected `X_Y` (underscore or pipe separator)"
            ));
        }
        let x: i32 = parts[0]
            .parse()
            .map_err(|_| format!("bad x `{}`", parts[0]))?;
        let y: i32 = parts[1]
            .parse()
            .map_err(|_| format!("bad y `{}`", parts[1]))?;
        Ok(Position::pixel(x, y))
    };
    match kind {
        "circle" => {
            let at = get_pos("at", "at")?;
            let color = get_color(Color::RED)?;
            let radius: i32 = kv
                .get("radius")
                .map(|s| s.parse().unwrap_or(30))
                .unwrap_or(30);
            let stroke: i32 = kv
                .get("stroke")
                .map(|s| s.parse().unwrap_or(3))
                .unwrap_or(3);
            Ok(Annotation::circle(at)
                .color(color)
                .radius(radius)
                .stroke(stroke)
                .build())
        }
        "arrow" => {
            let from = get_pos("from", "from")?;
            let to = get_pos("to", "to")?;
            let color = get_color(Color::BLUE)?;
            let stroke: i32 = kv
                .get("stroke")
                .map(|s| s.parse().unwrap_or(4))
                .unwrap_or(4);
            Ok(Annotation::arrow(from, to)
                .color(color)
                .stroke(stroke)
                .build())
        }
        "text" => {
            let at = get_pos("at", "at")?;
            let content = kv
                .get("content")
                .ok_or_else(|| "missing `content`".to_string())?
                .to_string();
            let color = get_color(Color::WHITE)?;
            let size: f32 = kv
                .get("size")
                .map(|s| s.parse().unwrap_or(24.0))
                .unwrap_or(24.0);
            Ok(Annotation::text(at, content)
                .color(color)
                .size(size)
                .build())
        }
        "box" => {
            let at = get_pos("at", "at")?;
            let width: i32 = kv
                .get("width")
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| "missing `width`".to_string())?;
            let height: i32 = kv
                .get("height")
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| "missing `height`".to_string())?;
            let color = get_color(Color::YELLOW)?;
            let stroke: i32 = kv
                .get("stroke")
                .map(|s| s.parse().unwrap_or(2))
                .unwrap_or(2);
            Ok(Annotation::box_(at, width, height)
                .color(color)
                .stroke(stroke)
                .build())
        }
        "line" => {
            let from = get_pos("from", "from")?;
            let to = get_pos("to", "to")?;
            let color = get_color(Color::CYAN)?;
            let stroke: i32 = kv
                .get("stroke")
                .map(|s| s.parse().unwrap_or(2))
                .unwrap_or(2);
            Ok(Annotation::line(from, to)
                .color(color)
                .stroke(stroke)
                .build())
        }
        other => Err(format!(
            "unknown annotation kind `{other}` (expected circle/arrow/text/box/line)"
        )),
    }
}
