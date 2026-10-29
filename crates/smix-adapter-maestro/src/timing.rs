//! How long a run took: before the first step, each step and the parts of
//! it the driver marked, the sub-steps under the step that ran them, and
//! after the last step.

use serde_json::{Value, json};
use smix_driver::phases::Phase;

/// One step's time, with the steps it ran under it.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct StepTiming {
    /// The step's number in the run, as the `STEP n:` lines count. `None`
    /// for a step inside `repeat`, `retry` or an inline `runFlow`, which
    /// those lines do not number.
    pub index: Option<usize>,
    /// What the step was, as the `STEP n:` lines say it.
    pub summary: String,
    /// `ok`, `skipped`, `expanded-subflow` or `failed`.
    pub verdict: &'static str,
    /// Milliseconds from the step starting to its answer.
    pub wall_ms: u64,
    /// The parts of the step the driver marked, in the order they ended.
    pub phases: Vec<Phase>,
    /// The steps this one ran: a `runFlow`'s, a `repeat`'s.
    pub steps: Vec<StepTiming>,
}

impl StepTiming {
    /// The wire spelling, the same in `--format json` and `run-summary.json`.
    pub fn to_json(&self) -> Value {
        json!({
            "index": self.index,
            "summary": self.summary,
            "verdict": self.verdict,
            "wallMs": self.wall_ms,
            "phases": self.phases.iter().map(|p| json!({
                "name": p.name,
                "startMs": p.start_ms,
                "ms": p.ms,
            })).collect::<Vec<_>>(),
            "steps": self.steps.iter().map(StepTiming::to_json).collect::<Vec<_>>(),
        })
    }
}

/// A whole flow's time.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct RunTiming {
    /// From the command starting on this flow to its result being written.
    pub total_ms: u64,
    /// Before the first step: parsing, finding the runner, opening the
    /// session, quieting animations.
    pub setup_ms: u64,
    /// After the last step: closing the session, putting animations back.
    pub teardown_ms: u64,
    /// The flow's own steps; each holds the steps it ran.
    pub steps: Vec<StepTiming>,
}

impl RunTiming {
    /// The wire spelling. `stepsMs` is the flow's own steps added up; what
    /// `totalMs` holds beyond setup, steps and teardown is time between
    /// steps.
    pub fn to_json(&self) -> Value {
        json!({
            "totalMs": self.total_ms,
            "setupMs": self.setup_ms,
            "stepsMs": self.steps.iter().map(|s| s.wall_ms).sum::<u64>(),
            "teardownMs": self.teardown_ms,
            "steps": self.steps.iter().map(StepTiming::to_json).collect::<Vec<_>>(),
        })
    }
}

/// The verdict word a step's result is reported by.
pub(crate) fn verdict_of(r: &Result<crate::RunStepReport, crate::RunError>) -> &'static str {
    match r {
        Ok(crate::RunStepReport::Ok) => "ok",
        Ok(crate::RunStepReport::Skipped { .. }) => "skipped",
        Ok(crate::RunStepReport::ExpandedSubflow { .. }) => "expanded-subflow",
        Err(_) => "failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(index: Option<usize>, ms: u64, steps: Vec<StepTiming>) -> StepTiming {
        StepTiming {
            index,
            summary: "tapOn".into(),
            verdict: "ok",
            wall_ms: ms,
            phases: Vec::new(),
            steps,
        }
    }

    #[test]
    fn steps_ms_adds_the_flows_own_steps_and_not_their_children() {
        let t = RunTiming {
            total_ms: 1000,
            setup_ms: 200,
            teardown_ms: 50,
            steps: vec![
                step(Some(1), 300, vec![step(Some(2), 250, vec![])]),
                step(Some(3), 100, vec![]),
            ],
        };
        let v = t.to_json();
        assert_eq!(v["stepsMs"], 400);
        assert_eq!(v["steps"][0]["steps"][0]["index"], 2);
        assert_eq!(v["steps"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn an_unnumbered_step_says_null_not_zero() {
        assert_eq!(step(None, 5, vec![]).to_json()["index"], Value::Null);
    }
}
