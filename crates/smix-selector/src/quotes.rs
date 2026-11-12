//! Curly and straight quotes compare equal.
//!
//! System text is typeset: Android's permission dialog says `Don’t allow`
//! and iOS's alerts `Don’t Allow`, both with U+2019, while a selector is
//! typed on a keyboard that gives `'`. Matched as written, `text: Don't
//! allow` found nothing on a screen showing exactly that. Both sides are
//! folded, so a selector copied from the screen still matches too.

use std::borrow::Cow;

fn plain(c: char) -> char {
    match c {
        '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' | '\u{2032}' => '\'',
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' | '\u{2033}' => '"',
        other => other,
    }
}

/// `s` with every typographic quote replaced by its straight form;
/// borrowed when there is none.
pub(crate) fn fold(s: &str) -> Cow<'_, str> {
    if s.chars().all(|c| plain(c) == c) {
        Cow::Borrowed(s)
    } else {
        Cow::Owned(s.chars().map(plain).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_without_a_quote_is_not_copied() {
        assert!(matches!(fold("Allow"), Cow::Borrowed(_)));
        assert_eq!(fold("\u{2018}a\u{2019} \u{201C}b\u{201D}"), "'a' \"b\"");
    }
}
