//! `--id x` / `--text y` and the rest, read as `id:x` / `text:y`.
//!
//! The MCP tools name an element `{"id": "x"}` and the CLI as `id:x`. A
//! caller moving between the two wrote `smix tap --text OK` and was told
//! only `For more information, try '--help'`. The flag forms are taken
//! here, before the command line is parsed, so the CLI keeps one
//! selector grammar: each becomes the `kind:value` it stands for.

/// The verbs that take one selector, and the flags that name one.
const VERBS: &[&str] = &["tap", "find", "wait-for", "fill", "scroll", "long-press"];
const FLAGS: &[(&str, &str)] = &[
    ("--id", "id"),
    ("--text", "text"),
    ("--label", "label"),
    ("--role", "role"),
    ("--ocr-text", "ocrText"),
];

/// `argv` with a selector given as a flag moved to where the selector
/// goes, as `kind:value`.
///
/// `fill` keeps its own `--text`, the text to type. Two selector flags,
/// or a flag beside a selector already written as `kind:value`, are
/// refused: which one was meant is a guess.
///
/// # Errors
///
/// What to write instead, when the command names its selector twice or
/// a flag has no value.
pub(crate) fn rewrite(argv: Vec<String>) -> Result<Vec<String>, String> {
    let Some(at) = verb_at(&argv) else {
        return Ok(argv);
    };
    let verb = argv[at].clone();
    let flags: Vec<&(&str, &str)> = FLAGS
        .iter()
        .filter(|(f, _)| !(verb == "fill" && *f == "--text"))
        .collect();
    let mut out: Vec<String> = argv[..=at].to_vec();
    let mut named: Vec<String> = Vec::new();
    let mut rest = argv[at + 1..].iter();
    while let Some(a) = rest.next() {
        let (name, inline) = match a.split_once('=') {
            Some((n, v)) => (n, Some(v.to_string())),
            None => (a.as_str(), None),
        };
        let Some((flag, kind)) = flags.iter().find(|(f, _)| *f == name) else {
            out.push(a.clone());
            continue;
        };
        let value = match inline.or_else(|| rest.next().cloned()) {
            Some(v) if !v.is_empty() => v,
            _ => {
                return Err(format!(
                    "`{flag}` needs a value: `smix {verb} {flag} <value>`"
                ));
            }
        };
        named.push(format!("{kind}:{value}"));
    }
    match named.as_slice() {
        [] => Ok(argv),
        [one] => {
            if let Some(written) = out[at + 1..].iter().find(|a| looks_like_a_selector(a)) {
                return Err(format!(
                    "`smix {verb}` was given its selector twice, `{written}` and `{one}`; \
                     give one of them"
                ));
            }
            out.insert(at + 1, one.clone());
            Ok(out)
        }
        several => Err(format!(
            "`smix {verb}` takes one selector and was given {}: {}; give one of them",
            several.len(),
            several.join(", ")
        )),
    }
}

/// Where the verb is: the first word that is not a flag or the value of
/// the one global flag that takes one.
fn verb_at(argv: &[String]) -> Option<usize> {
    let mut i = 1;
    while i < argv.len() {
        let a = &argv[i];
        if a == "--owner" {
            i += 2;
            continue;
        }
        if !a.starts_with('-') {
            return VERBS.contains(&a.as_str()).then_some(i);
        }
        i += 1;
    }
    None
}

fn looks_like_a_selector(a: &str) -> bool {
    !a.starts_with('-')
        && a.split_once(':')
            .is_some_and(|(k, _)| FLAGS.iter().any(|(_, kind)| *kind == k) || k == "point")
}

#[cfg(test)]
mod tests {
    use super::rewrite;
    use clap::Parser;

    fn argv(s: &str) -> Vec<String> {
        s.split(' ').map(str::to_string).collect()
    }

    #[test]
    fn a_selector_flag_is_read_as_its_shorthand() {
        assert_eq!(
            rewrite(argv("smix tap --text OK")).unwrap(),
            argv("smix tap text:OK")
        );
        assert_eq!(
            rewrite(argv("smix find --id=home-tab --device sim-a")).unwrap(),
            argv("smix find id:home-tab --device sim-a")
        );
        assert_eq!(
            rewrite(argv("smix --owner t tap --ocr-text Skip")).unwrap(),
            argv("smix --owner t tap ocrText:Skip")
        );
        assert_eq!(
            rewrite(argv("smix wait-for --role button --timeout 5")).unwrap(),
            argv("smix wait-for role:button --timeout 5")
        );
    }

    #[test]
    fn fill_keeps_its_own_text() {
        assert_eq!(
            rewrite(argv("smix fill --id email --text a@b.example")).unwrap(),
            argv("smix fill id:email --text a@b.example")
        );
    }

    #[test]
    fn a_selector_given_twice_is_refused() {
        let e = rewrite(argv("smix tap id:ok --text OK")).unwrap_err();
        assert!(e.contains("twice"), "{e}");
        let e = rewrite(argv("smix tap --id ok --text OK")).unwrap_err();
        assert!(e.contains("given 2"), "{e}");
        assert!(
            rewrite(argv("smix tap --id"))
                .unwrap_err()
                .contains("needs a value")
        );
    }

    #[test]
    fn other_commands_and_plain_shorthand_are_left_alone() {
        for s in [
            "smix tap id:ok",
            "smix sim list --json",
            "smix run flow.yaml --text x",
        ] {
            assert_eq!(rewrite(argv(s)).unwrap(), argv(s));
        }
    }

    #[test]
    fn the_rewritten_line_parses() {
        let line = rewrite(argv("smix tap --text OK --device sim-a")).unwrap();
        let cli = crate::Cli::try_parse_from(&line).expect("parses");
        assert!(matches!(cli.cmd, crate::Cmd::Tap { ref selector, .. } if selector == "text:OK"));
    }
}
