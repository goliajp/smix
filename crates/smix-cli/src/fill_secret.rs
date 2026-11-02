//! What `smix fill` types is never said back: not on success, not in a
//! refusal, and not when its own command line is wrong.

use clap::Parser;

/// Flags of `smix fill` whose value is not what is typed.
const SAFE_VALUES: &[&str] = &["--device", "--port", "--runner-port", "--platform"];

/// Parse the command line, or print clap's refusal and exit. A mistaken
/// `smix fill` (a forgotten `--text`) made clap print the text it took
/// for a stray argument — a password, on 2026-11-02. For `fill` every
/// value but the selector and the device's own flags is masked first.
pub(crate) fn parse_or_exit<C: Parser>() -> C {
    let argv: Vec<String> = std::env::args().collect();
    match C::try_parse_from(&argv) {
        Ok(c) => c,
        Err(e) if argv.get(1).map(String::as_str) == Some("fill") => {
            let said = e.render().to_string();
            eprint!("{}", mask_fill_values(&said, &argv[2..]));
            std::process::exit(e.exit_code());
        }
        Err(e) => e.exit(),
    }
}

/// `said` with every value of the `fill` arguments masked except the
/// selector (the first value) and the values of [`SAFE_VALUES`].
pub(crate) fn mask_fill_values(said: &str, args: &[String]) -> String {
    let mut out = said.to_string();
    let mut selector_seen = false;
    let mut after_safe_flag = false;
    for a in args {
        if a.starts_with('-') {
            let name = a.split('=').next().unwrap_or(a);
            if let Some((flag, value)) = a.split_once('=')
                && !SAFE_VALUES.contains(&flag)
                && !value.is_empty()
            {
                out = out.replace(value, smix_error::MASK);
            }
            after_safe_flag = SAFE_VALUES.contains(&name) && !a.contains('=');
            continue;
        }
        if after_safe_flag {
            after_safe_flag = false;
            continue;
        }
        if !selector_seen {
            selector_seen = true;
            continue;
        }
        out = out.replace(a.as_str(), smix_error::MASK);
    }
    out
}

/// What to type: `--text` as given, or the value of the environment
/// variable `--text-env` names, which keeps it off the command line and
/// out of shell history.
pub(crate) fn text_to_type(text: Option<String>, env: Option<String>) -> Result<String, String> {
    match (text, env) {
        (Some(t), None) => Ok(t),
        (None, Some(name)) => std::env::var(&name)
            .ok()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| {
                format!("--text-env {name}: the environment variable is not set (or is empty)")
            }),
        (Some(_), Some(_)) => Err("give --text or --text-env, not both".into()),
        (None, None) => Err("give --text, or --text-env naming the variable that holds it".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_stray_value_is_masked_and_the_selector_and_device_are_not() {
        let said = "error: unexpected argument 'hunter2' found (device emulator-5554, id:pw)";
        let a = args(&["id:pw", "hunter2", "--device", "emulator-5554"]);
        assert_eq!(
            mask_fill_values(said, &a),
            "error: unexpected argument '***' found (device emulator-5554, id:pw)"
        );
        let a = args(&["id:pw", "--text=hunter2", "--bogus"]);
        assert!(!mask_fill_values("x hunter2 y", &a).contains("hunter2"));
    }

    #[test]
    fn the_text_comes_from_the_flag_or_the_named_variable() {
        assert_eq!(text_to_type(Some("a".into()), None).as_deref(), Ok("a"));
        let e = text_to_type(None, Some("SMIX_TEST_UNSET_77Q".into())).unwrap_err();
        assert!(e.contains("SMIX_TEST_UNSET_77Q"), "{e}");
        assert!(text_to_type(None, None).is_err());
    }
}
