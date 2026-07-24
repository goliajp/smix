//! smix-authoring-propose — the fenced authoring-proposal tier.
//!
//! A failed flow run leaves a structured bundle on disk (`run-summary.json`,
//! `*.fail.tree.json`, `failure.json`, screenshots). This crate turns that
//! bundle, via a local `claude` CLI, into a [`Proposal`]: a machine-checkable
//! list of edits over the real `smix_adapter_maestro::Step` /
//! `smix_selector::Selector` vocabulary — never an invented predicate smix has
//! no verb for.
//!
//! It is an authoring aid, fenced the same way as `smix-ai-tier`: deletable,
//! opt-in, non-deterministic. Nothing that senses or acts depends on it.

use std::path::Path;

use serde::{Deserialize, Serialize};
use smix_adapter_maestro::Step;
use smix_ai_tier::AiTierConfig;
use smix_error::{ExpectationFailure, FailureCode, FailureInit};
use smix_selector::Selector;

/// A machine-checkable set of edits over a flow's `Vec<Step>`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Proposal {
    /// The proposed edits, in the order the model emitted them.
    pub edits: Vec<ProposalEdit>,
}

/// One structured edit. Internally tagged on `op`; the tag value is the
/// variant name lowerCamelCased (`replaceSelector`, `insertStep`,
/// `reorderStep`, `replaceStep`), while the payload fields stay snake_case to
/// mirror how `claude` naturally names them.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum ProposalEdit {
    /// Swap the selector of a selector-bearing step.
    ReplaceSelector {
        /// Index of the step whose selector to replace.
        step_index: usize,
        /// The replacement selector.
        new_selector: Selector,
    },
    /// Insert a new step before `before_index`. v1 policy restricts `step` to
    /// a wait verb (`extendedWaitUntil` / `waitForAnimationToEnd`).
    InsertStep {
        /// Position to insert before (`0..=flow_len`).
        before_index: usize,
        /// The step to insert.
        step: Step,
    },
    /// Move a step from one position to another.
    ReorderStep {
        /// Source index.
        from_index: usize,
        /// Destination index.
        to_index: usize,
    },
    /// Replace a whole step (verb change, or the expressible subset of
    /// assertion changes).
    ReplaceStep {
        /// Index of the step to replace.
        step_index: usize,
        /// The replacement step.
        new_step: Step,
    },
}

/// Why a [`Proposal`] failed [`Proposal::validate`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProposalError {
    /// An edit referenced a step index outside the flow.
    IndexOutOfRange {
        /// The offending index.
        index: usize,
        /// The flow length it was checked against.
        flow_len: usize,
    },
    /// An `InsertStep` carried a step that is not a wait verb.
    InsertNotWait {
        /// The verb the insertion actually carried.
        verb: &'static str,
    },
}

impl Proposal {
    /// Machine-check the v1 convergence policy: every index is in range
    /// (`InsertStep.before_index` may equal `flow_len`), and every
    /// `InsertStep` inserts a wait verb. Word-level vocabulary bounds on
    /// `ReplaceSelector` / `ReplaceStep` are enforced by serde already — a
    /// step or selector shape smix has no variant for never deserializes — so
    /// `validate` adds no predicate allow-list. `ReorderStep` is structurally
    /// well-formed once its indices are in range; whether a reorder is
    /// *effective* is a C4 device concern, not a schema one.
    pub fn validate(&self, flow_len: usize) -> Result<(), ProposalError> {
        for edit in &self.edits {
            match edit {
                ProposalEdit::ReplaceSelector { step_index, .. }
                | ProposalEdit::ReplaceStep { step_index, .. } => {
                    check_in_range(*step_index, flow_len)?;
                }
                ProposalEdit::ReorderStep { from_index, to_index } => {
                    check_in_range(*from_index, flow_len)?;
                    check_in_range(*to_index, flow_len)?;
                }
                ProposalEdit::InsertStep { before_index, step } => {
                    if *before_index > flow_len {
                        return Err(ProposalError::IndexOutOfRange {
                            index: *before_index,
                            flow_len,
                        });
                    }
                    match step {
                        Step::ExtendedWaitUntil { .. } | Step::WaitForAnimationToEnd { .. } => {}
                        other => {
                            return Err(ProposalError::InsertNotWait { verb: step_verb(other) });
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

fn check_in_range(index: usize, flow_len: usize) -> Result<(), ProposalError> {
    if index < flow_len {
        Ok(())
    } else {
        Err(ProposalError::IndexOutOfRange { index, flow_len })
    }
}

/// Pull a [`Proposal`] out of a model reply, tolerating prose around the JSON
/// object — the same forgiving extraction `smix-ai-tier` uses for verdicts.
pub fn parse_proposal_reply(reply: &str) -> Option<Proposal> {
    smix_ai_tier::parse_json_object::<Proposal>(reply)
}

/// Ask a local `claude` to read a failed flow's bundle and propose edits.
///
/// The prompt points `claude` (with `--tools Read`) at the original flow and
/// the on-disk bundle — `run-summary.json`, `failure.json`, any
/// `*.fail.tree.json`, any screenshots — and asks for one JSON object of
/// edits. A reply that is not a well-shaped [`Proposal`] is a driver error,
/// not an empty proposal: "the model answered but not in the shape asked for"
/// is a different claim from "there is nothing to improve".
///
/// The only external call is the local `claude` CLI (via [`smix_ai_tier::ask`]);
/// no network Claude API path is touched.
pub async fn propose_from_bundle(
    flow_path: &Path,
    bundle_dir: &Path,
    cfg: &AiTierConfig,
) -> Result<Proposal, ExpectationFailure> {
    let prompt = format!(
        "A smix test flow failed. Read the original flow at {flow} and the run \
         artifacts in {bundle}: run-summary.json (per-step verdicts), \
         failure.json (the structured failure with selector, suggestions, and \
         visible elements), any *.fail.tree.json (the accessibility tree at the \
         failing step), and any .png screenshots.\n\n\
         Propose the edits that would make the flow pass. Reply with one JSON \
         object and nothing else:\n\
         {{\"edits\": [ <edit>, ... ]}}\n\n\
         Each <edit> is exactly one of:\n\
         {{\"op\": \"replaceSelector\", \"step_index\": <n>, \"new_selector\": <selector>}}\n\
         {{\"op\": \"insertStep\", \"before_index\": <n>, \"step\": <step>}}   (step must be extendedWaitUntil or waitForAnimationToEnd)\n\
         {{\"op\": \"reorderStep\", \"from_index\": <n>, \"to_index\": <n>}}\n\
         {{\"op\": \"replaceStep\", \"step_index\": <n>, \"new_step\": <step>}}\n\n\
         A <selector> is a maestro selector object, e.g. {{\"id\": \"submit-btn\"}} \
         or {{\"text\": \"Login\"}}. A <step> is a maestro step object, e.g. \
         {{\"tapOn\": {{\"selector\": {{\"id\": \"submit-btn\"}}}}}} or \
         {{\"extendedWaitUntil\": {{\"selector\": {{\"id\": \"spinner\"}}, \"timeout_ms\": 5000, \"expect_visible\": false}}}}.",
        flow = flow_path.display(),
        bundle = bundle_dir.display(),
    );

    let reply = smix_ai_tier::ask(prompt, cfg).await?;

    parse_proposal_reply(&reply).ok_or_else(|| {
        driver_error(
            format!(
                "authoring-propose: no proposal in the reply — wanted a JSON object, got: {}",
                reply.trim()
            ),
            Some(
                "the model answered but not in the shape asked for; this is not \
                 an empty proposal"
                    .into(),
            ),
        )
    })
}

fn driver_error(message: String, hint: Option<String>) -> ExpectationFailure {
    ExpectationFailure::new(FailureInit {
        code: Some(FailureCode::DriverError),
        message,
        hint,
        ..Default::default()
    })
}

fn step_verb(step: &Step) -> &'static str {
    match step {
        Step::TapOn { .. } => "tapOn",
        Step::TapAtPoint { .. } => "tapAtPoint",
        Step::WebViewEval { .. } => "webViewEval",
        Step::WaitForAnimationToEnd { .. } => "waitForAnimationToEnd",
        Step::ExtendedWaitUntil { .. } => "extendedWaitUntil",
        Step::AssertVisible { .. } => "assertVisible",
        Step::InputText(_) => "inputText",
        Step::InputTextInto { .. } => "inputTextInto",
        Step::PressKey(_) => "pressKey",
        Step::Back => "back",
        Step::RunFlow(_) => "runFlow",
        Step::RunFlowConditional { .. } => "runFlowConditional",
        Step::RunFlowInline { .. } => "runFlowInline",
        Step::ScrollUntilVisible { .. } => "scrollUntilVisible",
        Step::EraseText(_) => "eraseText",
        Step::Swipe { .. } => "swipe",
        Step::LaunchApp { .. } => "launchApp",
        _ => "step",
    }
}
