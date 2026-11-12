//! `setEnvironment: { appearance?, textSize?, locale?, timeZone?, network? }`.

use serde_norway::Value;

use crate::{ParseError, Step};

pub(super) fn parse_set_environment(v: &Value) -> Result<Step, ParseError> {
    let bad = |reason: String| ParseError::InvalidValue {
        field: "setEnvironment".into(),
        reason,
    };
    let names = smix_lease::environment::commands::SETTINGS;
    let map = v.as_mapping().ok_or_else(|| {
        bad(format!(
            "expected a mapping of {}, got {v:?}",
            names.join(" / ")
        ))
    })?;
    for key in map.keys() {
        if !key.as_str().is_some_and(|k| names.contains(&k)) {
            return Err(bad(format!(
                "unknown key {key:?}; the settings are {}",
                names.join(", ")
            )));
        }
    }
    let mut settings = Vec::new();
    for name in names {
        match map.get(name) {
            None => {}
            Some(Value::String(s)) if !s.trim().is_empty() => {
                settings.push((name.to_string(), s.trim().to_string()));
            }
            Some(other) => return Err(bad(format!("{name} takes a word, got {other:?}"))),
        }
    }
    if settings.is_empty() {
        return Err(bad(format!("name at least one of {}", names.join(", "))));
    }
    Ok(Step::SetEnvironment { settings })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(yaml: &str) -> Result<Step, ParseError> {
        parse_set_environment(&serde_norway::from_str(yaml).expect("yaml"))
    }

    #[test]
    fn settings_come_in_the_order_they_are_applied() {
        let Step::SetEnvironment { settings } =
            parse("{ network: offline, appearance: dark, locale: ja-JP }").expect("ok")
        else {
            panic!("not setEnvironment")
        };
        let keys: Vec<&str> = settings.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["appearance", "locale", "network"]);
    }

    #[test]
    fn a_stray_key_an_empty_mapping_or_a_number_is_refused() {
        assert!(parse("{ theme: dark }").is_err());
        assert!(parse("{}").is_err());
        assert!(parse("{ textSize: 2 }").is_err());
        assert!(parse("dark").is_err());
    }
}
