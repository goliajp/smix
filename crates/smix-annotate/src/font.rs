//! v1.0 Phase C1 — bundled default fonts.
//!
//! smix-annotate ships two fonts embedded via `include_bytes!` so
//! text annotations work out of the box without any consumer-supplied
//! font path:
//!
//! - **Inter Regular** (~312 KB) — SIL Open Font License, from
//!   rsms/inter. Covers Latin + extended punctuation + Greek + Cyrillic.
//! - **Noto Sans SC subset** (~64 KB) — SIL Open Font License, from
//!   googlefonts/noto-cjk. Pre-subsetted to ~250 most-frequent CJK
//!   ideographs + ASCII + CJK punctuation. Covers typical QA/UI
//!   Chinese text in yaml annotations.
//!
//! # Routing
//!
//! `pick_font_for_codepoint(cp)` returns the appropriate font bytes
//! based on the Unicode range of `cp`:
//! - ASCII + Latin extended → Inter
//! - CJK Unified Ideographs + CJK punctuation → Noto Sans SC subset
//! - Everything else → Inter (fallback)
//!
//! For per-glyph routing, `Annotator::render` iterates chars and
//! picks the font per-codepoint, ensuring mixed-script text renders
//! correctly.
//!
//! # Overriding
//!
//! Callers can supply their own font via `Annotator::font(bytes)` —
//! that overrides both bundled fonts for the entire annotator's
//! render pass. Use for corpus-specific glyph coverage.

/// Inter Regular — Latin/Greek/Cyrillic + extended punctuation.
/// SIL Open Font License. From <https://github.com/rsms/inter>.
pub const INTER_REGULAR: &[u8] = include_bytes!("../fonts/inter-regular.ttf");

/// Noto Sans SC subset — top ~250 CJK ideographs + ASCII + CJK
/// punctuation. SIL Open Font License. Subsetted from
/// <https://github.com/googlefonts/noto-cjk>.
pub const NOTO_SANS_SC_SUBSET: &[u8] = include_bytes!("../fonts/noto-sans-sc-subset.ttf");

/// v1.0 Phase C1 — pick the bundled font best suited to render `cp`.
///
/// Precedence:
/// 1. CJK Unified Ideographs (U+4E00-U+9FFF), CJK Extension A
///    (U+3400-U+4DBF), CJK punctuation (U+3000-U+303F), or Halfwidth
///    and Fullwidth Forms (U+FF00-U+FFEF) → Noto Sans SC subset
/// 2. Everything else → Inter (fallback for unknown ranges too;
///    ab_glyph renders missing glyphs as tofu boxes rather than
///    panicking)
pub fn pick_font_for_codepoint(cp: u32) -> &'static [u8] {
    match cp {
        0x3000..=0x303F | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xFF00..=0xFFEF => {
            NOTO_SANS_SC_SUBSET
        }
        _ => INTER_REGULAR,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inter_font_nonempty() {
        assert!(INTER_REGULAR.len() > 10_000, "Inter font missing / empty");
    }

    #[test]
    fn cjk_font_nonempty() {
        assert!(
            NOTO_SANS_SC_SUBSET.len() > 10_000,
            "Noto Sans SC subset missing / empty"
        );
    }

    #[test]
    fn routing_latin_uses_inter() {
        assert_eq!(pick_font_for_codepoint('a' as u32), INTER_REGULAR);
        assert_eq!(pick_font_for_codepoint(' ' as u32), INTER_REGULAR);
    }

    #[test]
    fn routing_cjk_uses_noto_sc() {
        // 中 = U+4E2D — inside CJK Unified Ideographs.
        assert_eq!(pick_font_for_codepoint('中' as u32), NOTO_SANS_SC_SUBSET);
        // 登 = U+767B
        assert_eq!(pick_font_for_codepoint('登' as u32), NOTO_SANS_SC_SUBSET);
    }

    #[test]
    fn routing_cjk_punctuation() {
        // 、 = U+3001 (CJK ideographic comma).
        assert_eq!(pick_font_for_codepoint('、' as u32), NOTO_SANS_SC_SUBSET);
    }

    #[test]
    fn routing_unknown_falls_back_to_inter() {
        // Latin extended A range still routes to Inter (fallback).
        assert_eq!(pick_font_for_codepoint(0x1F600), INTER_REGULAR);
    }

    #[test]
    fn both_fonts_load_via_ab_glyph() {
        use ab_glyph::FontRef;
        FontRef::try_from_slice(INTER_REGULAR).expect("Inter loads");
        FontRef::try_from_slice(NOTO_SANS_SC_SUBSET).expect("Noto SC loads");
    }
}
