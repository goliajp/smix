//! Color parsing + palette.

use thiserror::Error;

/// RGBA color (0-255 per channel).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const RED: Color = Color::rgb(255, 0, 0);
    pub const GREEN: Color = Color::rgb(0, 200, 0);
    pub const BLUE: Color = Color::rgb(0, 100, 255);
    pub const YELLOW: Color = Color::rgb(255, 220, 0);
    pub const CYAN: Color = Color::rgb(0, 220, 220);
    pub const MAGENTA: Color = Color::rgb(220, 0, 220);
    pub const WHITE: Color = Color::rgb(255, 255, 255);
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    pub const ORANGE: Color = Color::rgb(255, 140, 0);
    pub const PURPLE: Color = Color::rgb(160, 32, 240);
    pub const PINK: Color = Color::rgb(255, 105, 180);
    pub const GRAY: Color = Color::rgb(128, 128, 128);

    /// Semantic — for "the expected value / anchor" markers.
    pub const EXPECTED: Color = Color::GREEN;
    /// Semantic — for "the observed / mismatched" markers.
    pub const ACTUAL: Color = Color::RED;
    /// Semantic — for hint / suggestion overlays.
    pub const HINT: Color = Color::YELLOW;
    /// Semantic — for error state.
    pub const ERROR: Color = Color::RED;
    /// Semantic — for success state.
    pub const SUCCESS: Color = Color::GREEN;

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub fn to_rgba(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Parse from a string spec — named color, `#RRGGBB`, `#RRGGBBAA`,
    /// or `rgba(r,g,b,a)`.
    pub fn parse(s: &str) -> Result<Self, ColorParseError> {
        let s = s.trim();
        if let Some(hex) = s.strip_prefix('#') {
            return parse_hex(hex);
        }
        if s.starts_with("rgb(") && s.ends_with(')') {
            return parse_rgb_fn(&s[4..s.len() - 1]);
        }
        if s.starts_with("rgba(") && s.ends_with(')') {
            return parse_rgba_fn(&s[5..s.len() - 1]);
        }
        parse_named(s)
    }
}

fn parse_hex(hex: &str) -> Result<Color, ColorParseError> {
    let bytes: Result<Vec<u8>, _> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
        .collect();
    let bytes = bytes.map_err(|_| ColorParseError::BadHex(hex.to_string()))?;
    match bytes.len() {
        3 => Ok(Color::rgb(bytes[0], bytes[1], bytes[2])),
        4 => Ok(Color::rgba(bytes[0], bytes[1], bytes[2], bytes[3])),
        _ => Err(ColorParseError::BadHex(hex.to_string())),
    }
}

fn parse_rgb_fn(inner: &str) -> Result<Color, ColorParseError> {
    let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
    if parts.len() != 3 {
        return Err(ColorParseError::BadRgb(inner.to_string()));
    }
    let r = parts[0]
        .parse()
        .map_err(|_| ColorParseError::BadRgb(inner.to_string()))?;
    let g = parts[1]
        .parse()
        .map_err(|_| ColorParseError::BadRgb(inner.to_string()))?;
    let b = parts[2]
        .parse()
        .map_err(|_| ColorParseError::BadRgb(inner.to_string()))?;
    Ok(Color::rgb(r, g, b))
}

fn parse_rgba_fn(inner: &str) -> Result<Color, ColorParseError> {
    let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
    if parts.len() != 4 {
        return Err(ColorParseError::BadRgb(inner.to_string()));
    }
    let r = parts[0]
        .parse()
        .map_err(|_| ColorParseError::BadRgb(inner.to_string()))?;
    let g = parts[1]
        .parse()
        .map_err(|_| ColorParseError::BadRgb(inner.to_string()))?;
    let b = parts[2]
        .parse()
        .map_err(|_| ColorParseError::BadRgb(inner.to_string()))?;
    let a_f: f32 = parts[3]
        .parse()
        .map_err(|_| ColorParseError::BadRgb(inner.to_string()))?;
    let a = (a_f.clamp(0.0, 1.0) * 255.0) as u8;
    Ok(Color::rgba(r, g, b, a))
}

fn parse_named(s: &str) -> Result<Color, ColorParseError> {
    match s.to_lowercase().as_str() {
        "red" => Ok(Color::RED),
        "green" => Ok(Color::GREEN),
        "blue" => Ok(Color::BLUE),
        "yellow" => Ok(Color::YELLOW),
        "cyan" => Ok(Color::CYAN),
        "magenta" => Ok(Color::MAGENTA),
        "white" => Ok(Color::WHITE),
        "black" => Ok(Color::BLACK),
        "orange" => Ok(Color::ORANGE),
        "purple" => Ok(Color::PURPLE),
        "pink" => Ok(Color::PINK),
        "gray" | "grey" => Ok(Color::GRAY),
        "expected" | "success" => Ok(Color::GREEN),
        "actual" | "error" => Ok(Color::RED),
        "hint" => Ok(Color::YELLOW),
        other => Err(ColorParseError::UnknownName(other.to_string())),
    }
}

#[derive(Debug, Error)]
pub enum ColorParseError {
    #[error("unknown color name: {0}")]
    UnknownName(String),
    #[error("bad hex color: {0}")]
    BadHex(String),
    #[error("bad rgb/rgba color: {0}")]
    BadRgb(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named() {
        assert_eq!(Color::parse("red").unwrap(), Color::RED);
        assert_eq!(Color::parse("EXPECTED").unwrap(), Color::GREEN);
    }

    #[test]
    fn hex_short() {
        assert_eq!(Color::parse("#FF0000").unwrap(), Color::RED);
        assert_eq!(Color::parse("#00CC00").unwrap(), Color::rgb(0, 204, 0));
    }

    #[test]
    fn hex_with_alpha() {
        assert_eq!(
            Color::parse("#FF000080").unwrap(),
            Color::rgba(255, 0, 0, 128)
        );
    }

    #[test]
    fn rgb_fn() {
        assert_eq!(Color::parse("rgb(255, 0, 0)").unwrap(), Color::RED);
    }

    #[test]
    fn rgba_fn() {
        let c = Color::parse("rgba(255, 0, 0, 0.5)").unwrap();
        assert_eq!(c.r, 255);
        assert_eq!(c.a, 127);
    }

    #[test]
    fn unknown_name_errors() {
        assert!(matches!(
            Color::parse("neverheard"),
            Err(ColorParseError::UnknownName(_))
        ));
    }
}
