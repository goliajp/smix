//! Baseline-relative perf regression detection.
//!
//! The 26 `perf_gate.rs` files hold absolute nanosecond ceilings — they
//! catch a spike but not slow drift under the ceiling, where +4% ten
//! times is +40% and never trips a single budget. This adds the
//! baseline-relative layer: compare a fresh measurement against a
//! committed baseline and flag anything slower than tolerance.
//!
//! The comparison is a pure function, deliberately separate from the
//! measurement. The measurement is machine-sensitive (M-series and CI
//! scheduler jitter); the verdict "is this delta a regression" is not,
//! and pinning the tests to the pure engine keeps the gate from being
//! flaky.

use std::collections::BTreeMap;
use std::hint::black_box;
use std::path::Path;
use std::time::Instant;

use serde::{Deserialize, Serialize};

/// Slowdown, in percent, tolerated before a metric is a regression.
pub const TOLERANCE_PCT: f64 = 5.0;

/// Iterations per metric. High enough that the median is stable on an
/// M-series; the pure gate logic is tested separately so this number
/// only affects measurement noise, not correctness.
const BENCH_ITERATIONS: u32 = 4000;

/// Named perf metrics, each a median measurement in nanoseconds.
///
/// A `BTreeMap` so iteration — and therefore the order of findings — is
/// deterministic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchSet {
    pub metrics: BTreeMap<String, f64>,
}

impl BenchSet {
    #[cfg(test)]
    fn of(pairs: &[(&str, f64)]) -> Self {
        BenchSet {
            metrics: pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
        }
    }
}

/// One way `current` diverges from `baseline`.
#[derive(Debug, Clone, PartialEq)]
pub enum Finding {
    /// A metric got slower beyond tolerance. Fails the gate.
    Regression {
        metric: String,
        baseline_ns: f64,
        current_ns: f64,
        pct: f64,
    },
    /// A baseline metric is absent from the current run. Fails the gate:
    /// a corpus that quietly stopped measuring something reads as a pass
    /// if only the metrics still present are compared.
    Missing { metric: String },
    /// A current metric has no baseline. Not a regression — there is
    /// nothing to regress against — but surfaced so `--update-baseline`
    /// gets run rather than the metric going ungated forever.
    Unbaselined { metric: String, current_ns: f64 },
}

impl Finding {
    /// Does this finding fail the gate? Regressions and disappearances
    /// do; an un-baselined new metric does not.
    #[must_use]
    pub fn is_failure(&self) -> bool {
        matches!(self, Finding::Regression { .. } | Finding::Missing { .. })
    }
}

/// Compare a fresh measurement against a committed baseline.
///
/// `tol_pct` is the slowdown, in percent, tolerated before a metric
/// counts as a regression. Only slowdowns are reported — a metric that
/// got faster is not a finding. Findings come out in metric-name order.
#[must_use]
pub fn compare(baseline: &BenchSet, current: &BenchSet, tol_pct: f64) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (metric, &base_ns) in &baseline.metrics {
        match current.metrics.get(metric) {
            None => findings.push(Finding::Missing {
                metric: metric.clone(),
            }),
            Some(&cur_ns) => {
                let pct = (cur_ns - base_ns) / base_ns * 100.0;
                if pct > tol_pct {
                    findings.push(Finding::Regression {
                        metric: metric.clone(),
                        baseline_ns: base_ns,
                        current_ns: cur_ns,
                        pct,
                    });
                }
            }
        }
    }
    for (metric, &cur_ns) in &current.metrics {
        if !baseline.metrics.contains_key(metric) {
            findings.push(Finding::Unbaselined {
                metric: metric.clone(),
                current_ns: cur_ns,
            });
        }
    }
    findings
}

/// Median per-iteration time in nanoseconds, after a warm-up.
fn median_ns(mut f: impl FnMut(), iters: u32) -> f64 {
    for _ in 0..(iters / 10).max(1) {
        f();
    }
    let mut samples = Vec::with_capacity(iters as usize);
    for _ in 0..iters {
        let t = Instant::now();
        f();
        samples.push(t.elapsed().as_nanos() as f64);
    }
    samples.sort_by(|a, b| a.partial_cmp(b).expect("no NaN timing"));
    samples[samples.len() / 2]
}

/// Measure the in-process corpus.
///
/// A small, deterministic, device-free set — real parser and selector
/// work that smix-cli already links. It is the seed; later checkpoints
/// widen it. What C1 proves is the measure → compare → gate pipeline,
/// not corpus breadth.
fn measure_corpus() -> BenchSet {
    use smix_selector::{Modifiers, Pattern, Selector, describe_selector};

    // smix's own bench workload, not a stand-in for a user's app — the
    // id is named for what it is so the placeholder gate (which forbids
    // com.example.* in production source) has nothing to catch.
    let flow_yaml = "\
appId: smix.bench.corpus
---
- launchApp
- tapOn: { id: sign-in }
- inputText: alice@example.com
- assertVisible: Welcome
- longPressOn: { id: row-3, duration: 1500 }
";
    let selector = Selector::Text {
        text: Pattern::text("Sign In"),
        modifiers: Modifiers::default(),
    };

    let mut metrics = BTreeMap::new();
    metrics.insert(
        "parse_flow_yaml.five_step".to_string(),
        median_ns(
            || {
                let _ = black_box(smix_adapter_maestro::parse_flow_yaml(black_box(flow_yaml)));
            },
            BENCH_ITERATIONS,
        ),
    );
    metrics.insert(
        "describe_selector.text".to_string(),
        median_ns(
            || {
                let _ = black_box(describe_selector(black_box(&selector)));
            },
            BENCH_ITERATIONS,
        ),
    );
    BenchSet { metrics }
}

fn load(path: &Path) -> Result<BenchSet, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("parse {}: {e}", path.display()))
}

fn write(path: &Path, set: &BenchSet) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    }
    let json = serde_json::to_string_pretty(set).map_err(|e| format!("serialize baseline: {e}"))?;
    std::fs::write(path, json + "\n").map_err(|e| format!("write {}: {e}", path.display()))
}

/// Run the gate: measure (or load a supplied current), compare against
/// the committed baseline, report, and fail on any regression or
/// disappeared metric.
pub fn run(
    update_baseline: bool,
    current_file: Option<&Path>,
    baseline_path: &Path,
) -> Result<(), String> {
    let current = match current_file {
        Some(p) => load(p)?,
        None => measure_corpus(),
    };

    if update_baseline {
        write(baseline_path, &current)?;
        println!(
            "smix bench: baseline updated ({} metrics) → {}",
            current.metrics.len(),
            baseline_path.display()
        );
        return Ok(());
    }

    let baseline = load(baseline_path).map_err(|e| {
        format!("{e} — no committed baseline; run `smix bench --update-baseline` first")
    })?;
    let findings = compare(&baseline, &current, TOLERANCE_PCT);

    for f in &findings {
        match f {
            Finding::Regression {
                metric,
                baseline_ns,
                current_ns,
                pct,
            } => {
                println!("REGRESSION {metric}: {baseline_ns:.1}ns → {current_ns:.1}ns (+{pct:.1}%)")
            }
            Finding::Missing { metric } => {
                println!("MISSING {metric}: in baseline, not measured this run")
            }
            Finding::Unbaselined { metric, current_ns } => {
                println!("NEW {metric}: {current_ns:.1}ns (no baseline; run --update-baseline)")
            }
        }
    }

    let failures = findings.iter().filter(|f| f.is_failure()).count();
    if failures == 0 {
        println!(
            "smix bench: {} metric(s) within {TOLERANCE_PCT:.0}% of baseline",
            current.metrics.len()
        );
        Ok(())
    } else {
        Err(format!(
            "smix bench: {failures} metric(s) regressed beyond {TOLERANCE_PCT:.0}%"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOL: f64 = 5.0;

    #[test]
    fn a_slowdown_within_tolerance_is_not_a_regression() {
        // 100 → 104 is 4%, under the 5% tolerance.
        let f = compare(
            &BenchSet::of(&[("m", 100.0)]),
            &BenchSet::of(&[("m", 104.0)]),
            TOL,
        );
        assert!(f.is_empty(), "4% should be tolerated: {f:?}");
    }

    #[test]
    fn a_slowdown_past_tolerance_is_a_regression_with_the_numbers() {
        let f = compare(
            &BenchSet::of(&[("m", 100.0)]),
            &BenchSet::of(&[("m", 106.0)]),
            TOL,
        );
        assert_eq!(f.len(), 1);
        match &f[0] {
            Finding::Regression {
                metric,
                baseline_ns,
                current_ns,
                pct,
            } => {
                assert_eq!(metric, "m");
                assert_eq!(*baseline_ns, 100.0);
                assert_eq!(*current_ns, 106.0);
                assert!((*pct - 6.0).abs() < 1e-9, "pct = {pct}");
            }
            other => panic!("expected Regression, got {other:?}"),
        }
        assert!(f[0].is_failure());
    }

    #[test]
    fn a_metric_that_got_faster_is_not_a_finding() {
        let f = compare(
            &BenchSet::of(&[("m", 100.0)]),
            &BenchSet::of(&[("m", 50.0)]),
            TOL,
        );
        assert!(f.is_empty(), "a speedup is not a finding: {f:?}");
    }

    #[test]
    fn a_baseline_metric_missing_from_current_fails_rather_than_passes() {
        let f = compare(
            &BenchSet::of(&[("kept", 100.0), ("dropped", 100.0)]),
            &BenchSet::of(&[("kept", 100.0)]),
            TOL,
        );
        assert_eq!(f.len(), 1);
        assert_eq!(
            f[0],
            Finding::Missing {
                metric: "dropped".into()
            }
        );
        assert!(f[0].is_failure(), "a disappeared metric must fail the gate");
    }

    #[test]
    fn a_new_metric_without_baseline_is_flagged_but_not_a_failure() {
        let f = compare(
            &BenchSet::of(&[("old", 100.0)]),
            &BenchSet::of(&[("old", 100.0), ("new", 42.0)]),
            TOL,
        );
        assert_eq!(f.len(), 1);
        assert_eq!(
            f[0],
            Finding::Unbaselined {
                metric: "new".into(),
                current_ns: 42.0
            }
        );
        assert!(
            !f[0].is_failure(),
            "a new metric has nothing to regress against"
        );
    }

    #[test]
    fn findings_come_out_in_metric_name_order() {
        let f = compare(
            &BenchSet::of(&[("b", 100.0), ("a", 100.0)]),
            &BenchSet::of(&[("b", 200.0), ("a", 200.0)]),
            TOL,
        );
        let names: Vec<&str> = f
            .iter()
            .filter_map(|x| match x {
                Finding::Regression { metric, .. } => Some(metric.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(names, ["a", "b"], "deterministic order");
    }
}
