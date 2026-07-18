/// Odd-length and non-ASCII hex used to slice past the end / mid-char and
/// panic, instead of returning the BadHex the type already defines. Both
/// reach here from yaml (`annotate_bridge`, where a parse failure is
/// supposed to warn and fall back to a default colour) and from the CLI
/// `--annotate` mini-DSL, so a panic takes down the whole flow.
#[test]
fn malformed_hex_is_an_error_not_a_panic() {
    for bad in ["#F00", "#12345", "#ff00ff0", "#zz", "#日本語色"] {
        assert!(
            smix_annotate::Color::parse(bad).is_err(),
            "{bad} should be a BadHex error"
        );
    }
}
