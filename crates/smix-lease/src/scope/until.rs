//! A holder's `until`: free text, where a leading `+45m` is a span that
//! starts when the scope is granted, not when it was asked for.

use chrono::{DateTime, Utc};

/// `+45m build` → `14:20 build` in local time, counted from `now`;
/// anything else as written. Also `+SECS`, `+90s` and `+Nh`.
pub fn until_label(raw: &str, now: DateTime<Utc>) -> String {
    let Some(rest) = raw.strip_prefix('+') else {
        return raw.to_string();
    };
    let (amount, note) = rest.split_once(' ').unwrap_or((rest, ""));
    let secs = if let Some(m) = amount.strip_suffix('m') {
        m.parse::<i64>().ok().map(|m| m * 60)
    } else if let Some(h) = amount.strip_suffix('h') {
        h.parse::<i64>().ok().map(|h| h * 3600)
    } else {
        amount
            .strip_suffix('s')
            .unwrap_or(amount)
            .parse::<i64>()
            .ok()
    };
    match secs {
        Some(s) => {
            let at = (now + chrono::Duration::seconds(s))
                .with_timezone(&chrono::Local)
                .format("%H:%M");
            format!("{at} {note}").trim_end().to_string()
        }
        None => raw.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_until_becomes_a_clock_time_and_keeps_its_note() {
        let t = DateTime::parse_from_rfc3339("2026-11-23T13:35:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let base = t.with_timezone(&chrono::Local).format("%H:%M").to_string();
        assert_ne!(until_label("+45m build", t), "+45m build");
        assert!(until_label("+45m build", t).ends_with(" build"));
        assert!(
            !until_label("+2h", t).contains(&base),
            "two hours on is another time"
        );
        assert_eq!(until_label("+90 x", t), until_label("+90s x", t));
        assert_eq!(until_label("23:30 nightly", t), "23:30 nightly");
        assert_eq!(until_label("+soon", t), "+soon");
    }
}
