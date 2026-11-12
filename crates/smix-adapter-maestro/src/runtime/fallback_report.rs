//! What a fallback chain that found nothing says about how long it looked.

/// How long a fallback chain looked, said so a reader can tell a chain that
/// waited from one that gave up: each layer is a tap that waits for its
/// target, and a chain with an OCR layer goes round again.
pub(super) fn chain_took(began: std::time::Instant, passes: u32) -> String {
    let rounds = if passes == 1 {
        "one pass".to_string()
    } else {
        format!("{passes} passes")
    };
    format!(
        "{} ms, {rounds}, each layer waiting as a tapOn does",
        began.elapsed().as_millis()
    )
}

/// A layer's own answer, without the words it says as the optional tap the
/// chain runs it as: the chain may not be optional, and a trace reading
/// "optional … skipped" sent a reader looking for a skipped step.
pub(super) fn layer_miss(reason: &str) -> &str {
    let r = reason.strip_prefix("optional ").unwrap_or(reason);
    r.strip_suffix("; skipped").unwrap_or(r)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_single_pass_is_said_as_one() {
        let s = super::chain_took(std::time::Instant::now(), 1);
        assert!(
            s.contains("one pass") && s.contains("each layer waiting as a tapOn does"),
            "{s}"
        );
        assert!(super::chain_took(std::time::Instant::now(), 3).contains("3 passes"));
    }

    #[test]
    fn a_layer_miss_loses_the_optional_wording() {
        assert_eq!(
            super::layer_miss("optional tapOn: element not found (id=btn-x); skipped"),
            "tapOn: element not found (id=btn-x)"
        );
        assert_eq!(
            super::layer_miss("tapOn anchored: anchor not found"),
            "tapOn anchored: anchor not found"
        );
    }
}
