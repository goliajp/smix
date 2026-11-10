//! "Did you mean …?" for a selector that matched nothing.

use crate::ElementSummary;

const SUGGESTION_THRESHOLD: f64 = 0.5;
const SUGGESTION_TOP_N: usize = 3;

/// Generate "Did you mean ...?" suggestions from the current visible
/// element list. Contract: threshold > 0.5, top 3, score desc →
/// field (name > text) → DFS index asc.
///
/// `target = None` → empty vec.
#[must_use]
pub fn build_suggestions(target: Option<&str>, visible: &[ElementSummary]) -> Vec<String> {
    let Some(target) = target else {
        return Vec::new();
    };
    let lower_target = target.to_lowercase();
    let mut out = led_by(target, visible);
    let mut candidates: Vec<(f64, &'static str, String, usize)> = Vec::new();
    for (i, el) in visible.iter().enumerate() {
        let mut best: Option<(f64, &'static str, String)> = None;
        if let Some(name) = &el.name
            && !name.is_empty()
        {
            let s = similarity(&name.to_lowercase(), &lower_target);
            if s > SUGGESTION_THRESHOLD {
                best = Some((s, "name", name.clone()));
            }
        }
        if let Some(id) = &el.id
            && !id.is_empty()
        {
            let s = similarity(&id.to_lowercase(), &lower_target);
            if s > SUGGESTION_THRESHOLD && best.as_ref().map(|(bs, _, _)| s > *bs).unwrap_or(true) {
                best = Some((s, "id", id.clone()));
            }
        }
        if let Some(text) = &el.text
            && !text.is_empty()
        {
            let s = similarity(&text.to_lowercase(), &lower_target);
            if s > SUGGESTION_THRESHOLD && best.as_ref().map(|(bs, _, _)| s > *bs).unwrap_or(true) {
                best = Some((s, "text", text.clone()));
            }
        }
        if let Some((score, field, value)) = best {
            candidates.push((score, field, value, i));
        }
    }
    // Sort: score desc → field name>text → index asc.
    let field_rank = |f: &str| match f {
        "name" => 0,
        "id" => 1,
        _ => 2, // text
    };
    candidates.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| field_rank(a.1).cmp(&field_rank(b.1)))
            .then_with(|| a.3.cmp(&b.3))
    });
    let near: Vec<String> = candidates
        .into_iter()
        .filter(|(_, _, value, _)| !out.iter().any(|o| o.contains(&format!("{value:?}"))))
        .take(SUGGESTION_TOP_N)
        .map(|(score, field, value, _)| {
            format!("Did you mean {value:?}? (similarity {score:.2}, field {field})")
        })
        .collect();
    out.extend(near);
    out
}

/// Elements whose name or text is what was asked for and then more after
/// a comma: a row that reads its title and its value as one accessibility
/// element ("Time Zone, JST"), where the title alone matches nothing
/// exactly. Offered first, with the way to match the title alone.
fn led_by(target: &str, visible: &[ElementSummary]) -> Vec<String> {
    let lower = target.to_lowercase();
    let mut out: Vec<String> = Vec::new();
    for said in visible
        .iter()
        .flat_map(|e| [e.name.as_deref(), e.text.as_deref()])
        .flatten()
    {
        let rest = said.to_lowercase();
        let Some(after) = rest.strip_prefix(&lower) else {
            continue;
        };
        if !after.trim_start().starts_with(',') {
            continue;
        }
        let line = format!(
            "Did you mean {said:?}? It starts with what you asked for: the element reads its \
             title and its value as one. Match the whole of it, or the title alone with \
             `text: {{ regex: \"^{}\\\\b\" }}`",
            regex_quote(target)
        );
        if !out.contains(&line) {
            out.push(line);
        }
        if out.len() == SUGGESTION_TOP_N {
            break;
        }
    }
    out
}

/// `s` with the characters a regular expression gives meaning to escaped.
fn regex_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if r"\.+*?()|[]{}^$".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Normalized `[0, 1]` string similarity. 1 = identical,
/// 0 = completely different.
#[must_use]
pub fn similarity(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    let (longer, shorter) = if a.chars().count() >= b.chars().count() {
        (a, b)
    } else {
        (b, a)
    };
    let llen = longer.chars().count();
    if llen == 0 {
        return 1.0;
    }
    let dist = edit_distance(longer, shorter);
    (llen as f64 - dist as f64) / llen as f64
}

/// Levenshtein edit distance (Wagner-Fischer).
#[must_use]
pub fn edit_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let a_len = a_chars.len();
    let b_len = b_chars.len();
    let mut dp: Vec<usize> = (0..=b_len).collect();
    for i in 1..=a_len {
        let mut prev = dp[0];
        dp[0] = i;
        for j in 1..=b_len {
            let tmp = dp[j];
            dp[j] = if a_chars[i - 1] == b_chars[j - 1] {
                prev
            } else {
                1 + dp[j].min(dp[j - 1]).min(prev)
            };
            prev = tmp;
        }
    }
    dp[b_len]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &str) -> ElementSummary {
        ElementSummary {
            role: None,
            name: Some(name.into()),
            id: None,
            text: None,
            bounds: smix_screen::Rect {
                x: 0.0,
                y: 0.0,
                w: 10.0,
                h: 10.0,
            },
            enabled: true,
        }
    }

    #[test]
    fn a_row_that_reads_title_and_value_as_one_is_offered_first() {
        let visible = [named("Language, English"), named("Time Zone, JST")];
        let s = build_suggestions(Some("Time Zone"), &visible);
        assert!(s[0].contains("\"Time Zone, JST\""), "{s:?}");
        assert!(s[0].contains(r#"regex: "^Time Zone\\b""#), "{s:?}");
        assert_eq!(
            s.iter().filter(|l| l.contains("Time Zone, JST")).count(),
            1,
            "offered once, not again as a near name: {s:?}"
        );
    }

    #[test]
    fn a_name_that_merely_starts_the_same_is_not_a_title_and_value() {
        let s = build_suggestions(Some("Time"), &[named("Timeline")]);
        assert!(
            s.iter()
                .all(|l| !l.contains("starts with what you asked for")),
            "{s:?}"
        );
    }

    #[test]
    fn the_title_is_quoted_for_the_regex() {
        assert_eq!(regex_quote("Plan (Pro)"), r"Plan \(Pro\)");
    }
}
