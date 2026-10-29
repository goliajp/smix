//! The JUnit XML a run writes with `--format junit`.

use std::path::Path;

/// One flow's `<testsuite>`, with one `<testcase>` whose `time` is the
/// flow's whole run in seconds. Failures embed the smix error message and
/// failure code in a `<failure>` sub-element.
///
/// The schema follows the surface-common Jenkins/GoCD JUnit variant
/// (widely accepted by CI dashboards, GitHub Actions, GitLab).
pub(crate) fn testsuite(
    flow: &Path,
    outcome: Result<&crate::RunReport, &crate::RunError>,
    total_ms: u64,
) -> String {
    let flow_name = flow
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("smix-flow");
    let (failures, xml_body, warnings_ct) = match outcome {
        Ok(report) => (0, String::new(), report.warnings.len()),
        Err(e) => {
            let msg = xml_escape(&format!("{e}"));
            let kind = xml_escape(&junit_failure_kind(e));
            let cdata = format!("{e}");
            (
                1,
                format!(
                    "      <failure type=\"{kind}\" message=\"{msg}\"><![CDATA[{cdata}]]></failure>\n",
                ),
                0,
            )
        }
    };
    let flow_esc = xml_escape(flow_name);
    let secs = seconds(total_ms);
    let mut out = String::new();
    out.push_str(&format!(
        "<testsuite name=\"smix\" tests=\"1\" failures=\"{failures}\" errors=\"0\" skipped=\"0\" time=\"{secs}\">\n"
    ));
    out.push_str(&format!(
        "  <testcase name=\"{flow_esc}\" classname=\"smix.flow\" time=\"{secs}\">\n"
    ));
    out.push_str(&xml_body);
    if warnings_ct > 0 {
        out.push_str(&format!(
            "      <system-err>{warnings_ct} warning(s) emitted</system-err>\n"
        ));
    }
    out.push_str("  </testcase>\n");
    out.push_str("</testsuite>\n");
    out
}

/// A JUnit document holding `suites`: the one suite as the root when
/// there is one, as a single flow's run always printed, and a
/// `<testsuites>` around them when there are several — a run of several
/// flows used to print one document per flow end to end, which no XML
/// reader takes.
pub fn junit_document(suites: &[String]) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    match suites {
        [one] => out.push_str(one),
        many => {
            out.push_str("<testsuites>\n");
            for s in many {
                out.push_str(s);
            }
            out.push_str("</testsuites>\n");
        }
    }
    out
}

fn seconds(ms: u64) -> String {
    format!("{}.{:03}", ms / 1000, ms % 1000)
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn junit_failure_kind(err: &crate::RunError) -> String {
    // Kind categorization for junit `<failure type=...>`. Match on
    // the RunError variant name via Debug for forward-compat with
    // new variants (unknown → smix.error).
    let dbg = format!("{err:?}");
    let leading = dbg.split_once('(').map(|(a, _)| a).unwrap_or(&dbg);
    format!("smix.{}", leading.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flows_time_is_its_whole_run_in_seconds() {
        let report = crate::RunReport::default();
        let x = testsuite(Path::new("flows/login.yaml"), Ok(&report), 12_345);
        assert!(
            x.contains(r#"<testcase name="login" classname="smix.flow" time="12.345">"#),
            "{x}"
        );
        assert!(x.contains(r#"time="12.345">"#), "{x}");
        assert_eq!(seconds(7), "0.007");
    }

    #[test]
    fn several_flows_are_one_document() {
        let report = crate::RunReport::default();
        let a = testsuite(Path::new("a.yaml"), Ok(&report), 1000);
        let b = testsuite(Path::new("b.yaml"), Ok(&report), 2000);
        let doc = junit_document(&[a.clone(), b]);
        assert_eq!(doc.matches("<?xml").count(), 1);
        assert!(doc.contains("<testsuites>\n<testsuite"), "{doc}");
        assert!(doc.trim_end().ends_with("</testsuites>"), "{doc}");
        let one = junit_document(&[a]);
        assert!(!one.contains("<testsuites>"), "{one}");
        assert_eq!(one.matches("<?xml").count(), 1);
    }
}
