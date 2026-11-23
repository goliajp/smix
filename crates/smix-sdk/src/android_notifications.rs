//! The notifications an Android device holds, from `dumpsys notification`.

/// One notification on the device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Posted {
    /// Android's own name for it, which posting it again keeps.
    pub key: String,
    pub package: String,
    pub title: String,
    pub body: String,
    /// Unix milliseconds it was posted at, when the dump says.
    pub at_ms: Option<i64>,
    /// Taking it opens it and takes it away.
    pub auto_cancel: bool,
    /// The heading of a group rather than a notification of its own.
    pub group_summary: bool,
}

/// The value of `name=value` in a record's first line, up to a space.
fn field<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let at = line.find(name)? + name.len();
    line[at..].split(' ').next()
}

/// `android.title=String (words)` → `words`. What is typed differently
/// (`SpannableString (…)`) reads the same; `null` is no words.
fn extra<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let rest = line.trim_start().strip_prefix(name)?.strip_prefix('=')?;
    let open = rest.find(" (")?;
    Some(&rest[open + 2..])
}

/// Whether a line in an `extras={` block starts the next extra.
fn next_extra(line: &str) -> bool {
    let t = line.trim_start();
    t == "}"
        || t.split_once('=')
            .is_some_and(|(k, _)| k.contains('.') && !k.contains(' '))
}

/// Every notification in `dumpsys notification --noredact`, in the order
/// the shade shows them.
#[must_use]
pub fn parse(dump: &str) -> Vec<Posted> {
    let mut out: Vec<Posted> = Vec::new();
    let mut lines = dump.lines().peekable();
    let mut inside = false;
    while let Some(line) = lines.next() {
        let indent = line.len() - line.trim_start().len();
        if indent <= 2 && !line.trim().is_empty() {
            inside = line.trim() == "Notification List:";
            continue;
        }
        if !inside {
            continue;
        }
        let t = line.trim_start();
        if t.starts_with("NotificationRecord(") {
            out.push(Posted {
                key: String::new(),
                package: field(t, "pkg=").unwrap_or_default().to_string(),
                title: String::new(),
                body: String::new(),
                at_ms: None,
                auto_cancel: false,
                group_summary: false,
            });
            continue;
        }
        let Some(n) = out.last_mut() else { continue };
        if let Some(key) = t.strip_prefix("key=") {
            n.key = key.trim().to_string();
        } else if let Some(flags) = t.strip_prefix("flags=") {
            n.auto_cancel = flags.contains("AUTO_CANCEL");
            n.group_summary = flags.contains("GROUP_SUMMARY");
        } else if let Some(ms) = t.strip_prefix("mCreationTimeMs=") {
            n.at_ms = ms.trim().parse().ok();
        } else {
            for (name, into) in [
                ("android.title", &mut n.title),
                ("android.text", &mut n.body),
            ] {
                let Some(first) = extra(line, name) else {
                    continue;
                };
                // words with line breaks run on until the next extra
                let mut words = first.to_string();
                while lines.peek().is_some_and(|l| !next_extra(l)) {
                    words.push('\n');
                    words.push_str(lines.next().unwrap_or_default());
                }
                *into = words.strip_suffix(')').unwrap_or(&words).to_string();
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Android 16, three posted by the fixture, its group's heading, one of
    // the system's, and a list after it that is not the shade's
    const DUMP: &str = include_str!("../tests/fixtures/dumpsys-notification.txt");

    #[test]
    fn every_posted_notification_reads_with_its_words() {
        let all = parse(DUMP);
        let ours: Vec<(&str, &str, bool)> = all
            .iter()
            .filter(|n| n.package == "dev.smix.fixture")
            .map(|n| (n.title.as_str(), n.body.as_str(), n.group_summary))
            .collect();
        assert_eq!(
            ours,
            [
                ("", "", true),
                ("T4 你好 (x)", "B 4\nsecond = line", false),
                ("T3 你好", "B 3", false),
            ]
        );
        assert!(
            all.iter()
                .any(|n| n.package == "android" && !n.title.is_empty())
        );
        assert!(all.iter().all(|n| !n.key.is_empty()), "{all:?}");
        let t3 = all.iter().find(|n| n.title == "T3 你好").unwrap();
        assert_eq!(t3.key, "0|dev.smix.fixture|175061247|null|10500");
        assert!(t3.auto_cancel);
        assert_eq!(t3.at_ms, Some(1_795_471_390_010));
    }

    #[test]
    fn a_list_that_is_not_the_shades_is_not_read() {
        assert!(!parse(DUMP).iter().any(|n| n.title == "ENQUEUED"));
        assert!(parse("").is_empty());
    }
}
