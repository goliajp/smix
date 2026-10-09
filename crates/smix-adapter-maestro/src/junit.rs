//! The JUnit XML a run writes with `--format junit`.

use std::path::Path;

/// Emit a JUnit XML `<testsuite>` with one `<testcase>` per yaml flow.
/// Failures embed the smix error message + failure code in a
/// `<failure>` sub-element.
///
/// The schema follows the surface-common Jenkins/GoCD JUnit variant
/// (widely accepted by CI dashboards, GitHub Actions, GitLab).
pub(crate) fn emit_junit(flow: &Path, outcome: Result<&crate::RunReport, &crate::RunError>) {
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
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<testsuite name=\"smix\" tests=\"1\" failures=\"{failures}\" errors=\"0\" skipped=\"0\">\n"
    ));
    out.push_str(&format!(
        "  <testcase name=\"{flow_esc}\" classname=\"smix.flow\" time=\"0\">\n"
    ));
    out.push_str(&xml_body);
    if warnings_ct > 0 {
        out.push_str(&format!(
            "      <system-err>{warnings_ct} warning(s) emitted</system-err>\n"
        ));
    }
    out.push_str("  </testcase>\n");
    out.push_str("</testsuite>\n");
    println!("{out}");
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
