//! The verdict, written for whoever reads it next — which is usually
//! not a person.
//!
//! Every line names the requirement, the sentence it stands for, what
//! is missing and where the claims were read. That is what somebody
//! acts on. A percentage is not: it turns this into a score, and a
//! score is met by writing claims rather than by covering anything.
//!
//! The word is `claimed` throughout. Nothing here says a requirement is
//! verified, and the rendering is exactly where that would slip.

use crate::{Reconciliation, Regression};
use std::fmt::Write as _;

/// Render a reconciliation and whatever has been lost since the
/// baseline.
///
/// Regressions first: they are the thing to act on, and what merely
/// *is* can be read afterwards.
pub fn render(r: &Reconciliation, regressions: &[Regression]) -> String {
    let mut out = String::new();
    let total = r.unclaimed.len() + r.partially_claimed.len() + r.fully_claimed.len();

    if !regressions.is_empty() {
        let _ = writeln!(out, "lost since the baseline:");
        for reg in regressions {
            let _ = writeln!(out, "  {reg}");
        }
        let _ = writeln!(out);
    }

    if !r.unclaimed.is_empty() {
        let _ = writeln!(out, "claimed by nobody:");
        for c in &r.unclaimed {
            let _ = writeln!(out, "  {} — {}", c.id, c.statement);
            let _ = writeln!(out, "      read from {}", c.origin);
        }
        let _ = writeln!(out);
    }

    if !r.partially_claimed.is_empty() {
        let _ = writeln!(out, "claimed by some and not all:");
        for p in &r.partially_claimed {
            let _ = writeln!(out, "  {} — {}", p.contract.id, p.contract.statement);
            let _ = writeln!(
                out,
                "      claimed by {}, missing {}",
                p.claimed_by.join(", "),
                p.missing.join(", ")
            );
            for origin in &p.origins {
                let _ = writeln!(out, "      claim read from {origin}");
            }
        }
        let _ = writeln!(out);
    }

    let _ = writeln!(
        out,
        "{total} requirement(s): {} claimed by every expected platform, \
         {} by some, {} by nobody.",
        r.fully_claimed.len(),
        r.partially_claimed.len(),
        r.unclaimed.len()
    );
    let _ = writeln!(
        out,
        "A claim says a suite means to cover a requirement. It does not say \
         the test is good, that it passed, or that two platforms check the \
         same thing."
    );
    out
}
