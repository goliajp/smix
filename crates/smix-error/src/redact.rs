//! A failure with a secret taken out of everything it says.

use crate::ExpectationFailure;

/// What a secret is shown as.
pub const MASK: &str = "***";

/// Pieces of a secret shorter than this are left alone where they stand
/// by themselves: two characters of a password say nothing, and masking
/// every "ab" in a message would make it unreadable.
const SHORTEST_PIECE: usize = 3;

impl ExpectationFailure {
    /// This failure with `secret` masked wherever it says anything: the
    /// message, the hint, suggestions, device log lines, and the text and
    /// names of the elements it lists. A runner that quotes what a field
    /// held ("holds \"un1-first\"") quotes part of what was typed, so a
    /// quoted piece of the secret is masked too.
    #[must_use]
    pub fn without_secret(mut self, secret: &str) -> Self {
        if secret.is_empty() {
            return self;
        }
        self.message = mask(&self.message, secret);
        self.hint = self.hint.map(|h| mask(&h, secret));
        for s in &mut self.suggestions {
            *s = mask(s, secret);
        }
        for line in &mut self.device_log {
            *line = mask(line, secret);
        }
        for e in &mut self.visible_elements {
            e.text = e.text.as_deref().map(|t| mask_field(t, secret));
            e.name = e.name.as_deref().map(|n| mask_field(n, secret));
        }
        self
    }
}

/// `text` with every whole `secret`, and every quoted piece of it, masked.
pub fn mask(text: &str, secret: &str) -> String {
    let whole = text.replace(secret, MASK);
    let mut out = String::with_capacity(whole.len());
    let mut rest = whole.as_str();
    while let Some(open) = rest.find('"') {
        out.push_str(&rest[..=open]);
        rest = &rest[open + 1..];
        let Some(close) = rest.find('"') else {
            break;
        };
        let quoted = &rest[..close];
        if is_piece(quoted, secret) {
            out.push_str(MASK);
        } else {
            out.push_str(quoted);
        }
        out.push('"');
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out
}

/// An element's own text: masked whole when it is the secret or a piece
/// of it, otherwise with the secret masked inside it.
fn mask_field(text: &str, secret: &str) -> String {
    if is_piece(text, secret) {
        MASK.to_string()
    } else {
        text.replace(secret, MASK)
    }
}

fn is_piece(s: &str, secret: &str) -> bool {
    s.chars().count() >= SHORTEST_PIECE.min(secret.chars().count())
        && !s.is_empty()
        && secret.contains(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FailureCode, FailureInit};

    #[test]
    fn the_secret_and_quoted_pieces_of_it_are_masked() {
        let said = r#"input-text: typed "run1-first" into f in 1 chunk(s); the field held "" and holds "un1-first"."#;
        assert_eq!(
            mask(said, "run1-first"),
            r#"input-text: typed "***" into f in 1 chunk(s); the field held "" and holds "***"."#
        );
        // a quoted word that is not a piece of the secret stays
        assert_eq!(
            mask(r#"id "input-password""#, "hunter2"),
            r#"id "input-password""#
        );
        // a piece shorter than three characters stays where it stands alone
        assert_eq!(mask(r#"held "hu""#, "hunter2"), r#"held "hu""#);
        // a short secret is masked whole
        assert_eq!(mask("typed ab here", "ab"), "typed *** here");
    }

    #[test]
    fn every_part_of_a_failure_loses_the_secret() {
        let mut e = ExpectationFailure::new(FailureInit {
            code: Some(FailureCode::DriverError),
            message: "fill sent hunter2".into(),
            hint: Some("the field holds \"unter2\"".into()),
            ..Default::default()
        });
        e.suggestions = vec!["type hunter2 again".into()];
        e.device_log = vec!["log: hunter2".into()];
        let e = e.without_secret("hunter2");
        let all = format!(
            "{} {:?} {:?} {:?}",
            e.message, e.hint, e.suggestions, e.device_log
        );
        assert!(!all.contains("hunter2") && !all.contains("unter2"), "{all}");
        assert_eq!(e.message, "fill sent ***");
    }
}
