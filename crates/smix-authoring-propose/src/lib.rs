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
use smix_error::{ExpectationFailure, FailureCode, FailureInit};
use smix_selector::Selector;

/// Re-exported so a CLI wire needs only this crate as a dependency, not
/// `smix-ai-tier` directly — keeping the authoring-propose fence's allowlist
/// to a single crate.
pub use smix_ai_tier::AiTierConfig;

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
                ProposalEdit::ReorderStep {
                    from_index,
                    to_index,
                } => {
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
                            return Err(ProposalError::InsertNotWait {
                                verb: step_verb(other),
                            });
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
         The single most reliable repair signal is the `suggestions` array in \
         failure.json (the driver's \"Did you mean ...?\" list for a selector \
         that matched nothing); prefer swapping the failing selector to a \
         suggested one. `*.fail.tree.json` and .png may be absent — ignore them \
         if missing rather than treating their absence as an error.\n\n\
         Each <edit> is exactly one of:\n\
         {{\"op\": \"replaceSelector\", \"step_index\": <n>, \"new_selector\": <selector>}}\n\
         {{\"op\": \"insertStep\", \"before_index\": <n>, \"step\": <step>}}   (step must be extendedWaitUntil or waitForAnimationToEnd)\n\
         {{\"op\": \"reorderStep\", \"from_index\": <n>, \"to_index\": <n>}}\n\
         {{\"op\": \"replaceStep\", \"step_index\": <n>, \"new_step\": <step>}}\n\n\
         Every index (`step_index`, `before_index`, `from_index`, `to_index`) \
         is 0-based into the flow's step list: the first step is 0. Do not use \
         the 1-based `n` from run-summary.json's `steps[].n`.\n\n\
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

/// Why [`apply`] could not turn a [`Proposal`] into an amended flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplyError {
    /// The proposal failed [`Proposal::validate`] (index out of range, or
    /// an `InsertStep` carrying a non-wait verb).
    Invalid(ProposalError),
    /// A `ReplaceSelector` targeted a step that carries no selector.
    /// Refusing beats a silent no-op: the model asked to change a selector
    /// on a step that has none, which is a malformed proposal, not a
    /// change to drop on the floor.
    NoSelector {
        /// The targeted step index.
        step_index: usize,
        /// The verb of the step that has no selector.
        verb: &'static str,
    },
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApplyError::Invalid(e) => write!(f, "proposal failed validation: {e:?}"),
            ApplyError::NoSelector { step_index, verb } => write!(
                f,
                "replaceSelector at step {step_index} targets `{verb}`, which has no selector"
            ),
        }
    }
}

impl std::error::Error for ApplyError {}

/// Validate a [`Proposal`] then apply its edits to a copy of `steps`,
/// returning the amended flow. Validation runs first (index bounds +
/// insert-wait policy); a failure propagates as [`ApplyError::Invalid`]
/// rather than a partial mutation. Edits apply in the order the model
/// emitted them.
///
/// Device-free: a pure `Vec<Step>` transform, no emit / parse / claude /
/// sim involved.
///
/// # Errors
///
/// [`ApplyError::Invalid`] when [`Proposal::validate`] rejects the
/// proposal; [`ApplyError::NoSelector`] when a `ReplaceSelector` targets a
/// step with no selector field.
pub fn apply(proposal: &Proposal, steps: &[Step]) -> Result<Vec<Step>, ApplyError> {
    proposal
        .validate(steps.len())
        .map_err(ApplyError::Invalid)?;
    let mut out = steps.to_vec();
    for edit in &proposal.edits {
        match edit {
            ProposalEdit::ReplaceSelector {
                step_index,
                new_selector,
            } => {
                set_step_selector(&mut out[*step_index], new_selector.clone()).map_err(|verb| {
                    ApplyError::NoSelector {
                        step_index: *step_index,
                        verb,
                    }
                })?;
            }
            ProposalEdit::InsertStep { before_index, step } => {
                out.insert(*before_index, step.clone());
            }
            ProposalEdit::ReorderStep {
                from_index,
                to_index,
            } => {
                let s = out.remove(*from_index);
                out.insert(*to_index, s);
            }
            ProposalEdit::ReplaceStep {
                step_index,
                new_step,
            } => {
                out[*step_index] = new_step.clone();
            }
        }
    }
    Ok(out)
}

fn set_step_selector(step: &mut Step, new_selector: Selector) -> Result<(), &'static str> {
    match step {
        Step::TapOn { selector, .. }
        | Step::ExtendedWaitUntil { selector, .. }
        | Step::AssertVisible { selector }
        | Step::AssertNotVisible { selector }
        | Step::InputTextInto { selector, .. }
        | Step::ScrollUntilVisible { selector, .. }
        | Step::CopyTextFrom { selector }
        | Step::DoubleTapOn { selector }
        | Step::RepeatTap { selector, .. }
        | Step::LongPressOn { selector, .. } => {
            *selector = new_selector;
            Ok(())
        }
        other => Err(step_verb(other)),
    }
}

fn driver_error(message: String, hint: Option<String>) -> ExpectationFailure {
    ExpectationFailure::new(FailureInit {
        code: Some(FailureCode::DriverError),
        message,
        hint,
        ..Default::default()
    })
}

/// Why [`propose_and_amend`] could not turn a failed flow + its bundle into an
/// amended flow yaml. Each arm names the stage that failed; nothing is
/// swallowed and no stage falls back to an empty amend.
#[derive(Debug)]
pub enum AmendError {
    /// The flow file could not be read.
    ReadFlow(std::io::Error),
    /// The flow yaml did not parse.
    ParseFlow(String),
    /// The local `claude` step failed (CLI error, or a reply that was not a
    /// well-shaped [`Proposal`]). Boxed: an `ExpectationFailure` is ~10× the
    /// size of the other arms, so an unboxed variant bloats every `AmendError`.
    Propose(Box<ExpectationFailure>),
    /// The proposal could not be applied to the flow's steps.
    Apply(ApplyError),
    /// The amended steps could not be emitted back to maestro yaml.
    Emit(smix_adapter_maestro::EmitError),
}

/// Glue the C2/C3 pieces into one device-free primitive: read `flow_path`,
/// parse it, ask a local `claude` to propose edits from the on-disk `bundle_dir`,
/// apply them, and emit the amended flow as maestro yaml.
///
/// The only external call is the local `claude` CLI (through
/// [`propose_from_bundle`]); no network Claude API path is touched. Every stage
/// error maps to its own [`AmendError`] arm — a failing claude surfaces, it
/// never collapses to an empty amend.
pub async fn propose_and_amend(
    flow_path: &Path,
    bundle_dir: &Path,
    cfg: &AiTierConfig,
) -> Result<String, AmendError> {
    let yaml = std::fs::read_to_string(flow_path).map_err(AmendError::ReadFlow)?;
    let flow = smix_adapter_maestro::parse_flow_yaml(&yaml)
        .map_err(|e| AmendError::ParseFlow(e.to_string()))?;

    let proposal = propose_from_bundle(flow_path, bundle_dir, cfg)
        .await
        .map_err(|e| AmendError::Propose(Box::new(e)))?;

    let amended = apply(&proposal, &flow.steps).map_err(AmendError::Apply)?;
    smix_adapter_maestro::emit_flow_yaml(&amended, &flow.app_id).map_err(AmendError::Emit)
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
