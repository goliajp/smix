//! `Step` → maestro yaml — the inverse of [`crate::parse_flow_yaml`].
//!
//! Hand-built like the parser: each variant produces the maestro-form
//! yaml AST node the parser reads back, so a `Vec<Step>` round-trips to
//! itself. `Step`'s camelCase serde form is NOT the parser's read shape
//! (`tapOn`'s scalar/map split, `inputText`'s two arms, the
//! `visible`/`notVisible` selection), so serde cannot round-trip for
//! free — the shapes are constructed here to match the parser 1:1.
//!
//! Coverage is the round-trip core set. A variant outside it, or a
//! selector maestro yaml has no faithful spelling for, is refused with
//! [`EmitError::Unsupported`] rather than emitted into yaml that would
//! parse back to something different — the same discipline
//! `smix-recorder`'s `generate_maestro_yaml` uses.

use crate::Step;
use serde_norway::{Mapping, Number, Value};
use smix_selector::{Modifiers, Pattern, Selector};

/// Why a `Vec<Step>` could not be emitted to faithful maestro yaml.
#[derive(Debug, thiserror::Error)]
pub enum EmitError {
    /// A step variant, or a selector form it carries, has no maestro yaml
    /// spelling that parses back to the same value. `verb` names the
    /// offending step or selector form.
    #[error("cannot emit `{verb}` as maestro yaml that round-trips faithfully")]
    Unsupported {
        /// The step verb or selector form that could not be emitted.
        verb: &'static str,
    },
    /// The yaml serializer failed on the assembled AST.
    #[error("yaml serialize failed: {0}")]
    Serialize(String),
}

/// Emit a maestro yaml flow that [`crate::parse_flow_yaml`] parses back
/// into the same `steps`. `app_id` becomes the `appId:` header.
///
/// # Errors
///
/// [`EmitError::Unsupported`] when a step (or a selector it carries) is
/// outside the round-trip core set; [`EmitError::Serialize`] on a yaml
/// writer failure.
pub fn emit_flow_yaml(steps: &[Step], app_id: &str) -> Result<String, EmitError> {
    let mut body: Vec<Value> = Vec::with_capacity(steps.len());
    for step in steps {
        body.push(emit_step(step)?);
    }

    let mut header = Mapping::new();
    header.insert(
        Value::String("appId".to_string()),
        Value::String(app_id.to_string()),
    );
    let header_str = serde_norway::to_string(&Value::Mapping(header))
        .map_err(|e| EmitError::Serialize(e.to_string()))?;
    let header_str = header_str.trim_end();
    let body_str =
        serde_norway::to_string(&body).map_err(|e| EmitError::Serialize(e.to_string()))?;
    let body_str = body_str.trim_end();

    Ok(format!("{header_str}\n---\n{body_str}\n"))
}

fn emit_step(step: &Step) -> Result<Value, EmitError> {
    match step {
        Step::TapOn {
            selector,
            optional,
            dispatch,
        } => {
            if *optional || dispatch.is_some() {
                return Err(EmitError::Unsupported { verb: "tapOn" });
            }
            Ok(single("tapOn", selector_to_maestro(selector)?))
        }
        Step::InputText(s) => Ok(single("inputText", Value::String(s.clone()))),
        Step::InputTextInto { selector, text } => {
            let id = match selector {
                Selector::Id { id, modifiers } if is_default_modifiers(modifiers) => id.clone(),
                _ => return Err(EmitError::Unsupported { verb: "inputText" }),
            };
            let mut inner = Mapping::new();
            inner.insert(Value::String("id".into()), Value::String(id));
            inner.insert(Value::String("text".into()), Value::String(text.clone()));
            Ok(single("inputText", Value::Mapping(inner)))
        }
        Step::AssertVisible { selector } => {
            Ok(single("assertVisible", selector_to_maestro(selector)?))
        }
        Step::AssertNotVisible { selector } => {
            Ok(single("assertNotVisible", selector_to_maestro(selector)?))
        }
        Step::ExtendedWaitUntil {
            selector,
            timeout_ms,
            expect_visible,
        } => {
            let arm = if *expect_visible { "visible" } else { "notVisible" };
            let mut inner = Mapping::new();
            inner.insert(
                Value::String(arm.into()),
                selector_to_maestro(selector)?,
            );
            inner.insert(
                Value::String("timeout".into()),
                Value::Number(Number::from(*timeout_ms)),
            );
            Ok(single("extendedWaitUntil", Value::Mapping(inner)))
        }
        Step::WaitForAnimationToEnd { ceiling_ms } => {
            let mut inner = Mapping::new();
            inner.insert(
                Value::String("timeout".into()),
                Value::Number(Number::from(*ceiling_ms)),
            );
            Ok(single("waitForAnimationToEnd", Value::Mapping(inner)))
        }
        Step::Back => Ok(Value::String("back".into())),
        Step::PressKey(s) => Ok(single("pressKey", Value::String(s.clone()))),
        Step::EraseText(n) => Ok(single("eraseText", Value::Number(Number::from(*n)))),
        Step::Swipe { from, to } => {
            let mut inner = Mapping::new();
            inner.insert(Value::String("from".into()), xy(*from));
            inner.insert(Value::String("to".into()), xy(*to));
            Ok(single("swipe", Value::Mapping(inner)))
        }
        Step::ScrollUntilVisible {
            selector,
            direction,
        } => {
            let mut inner = Mapping::new();
            inner.insert(
                Value::String("element".into()),
                selector_to_maestro(selector)?,
            );
            inner.insert(
                Value::String("direction".into()),
                Value::String(direction.clone()),
            );
            Ok(single("scrollUntilVisible", Value::Mapping(inner)))
        }
        Step::StopApp => Ok(Value::String("stopApp".into())),
        Step::HideKeyboard => Ok(Value::String("hideKeyboard".into())),
        Step::Scroll => Ok(Value::String("scroll".into())),
        Step::LaunchApp {
            app_id,
            clear_state,
            clear_keychain,
            permissions,
            arguments,
            stop_app,
            wait_for_interactive_ms,
        } => {
            let mut inner = Mapping::new();
            inner.insert(
                Value::String("appId".into()),
                Value::String(app_id.clone()),
            );
            inner.insert(Value::String("clearState".into()), Value::Bool(*clear_state));
            inner.insert(
                Value::String("clearKeychain".into()),
                Value::Bool(*clear_keychain),
            );
            inner.insert(Value::String("stopApp".into()), Value::Bool(*stop_app));
            if !arguments.is_empty() {
                inner.insert(
                    Value::String("arguments".into()),
                    Value::Sequence(arguments.iter().map(|a| Value::String(a.clone())).collect()),
                );
            }
            if !permissions.is_empty() {
                let mut perms = Mapping::new();
                for (name, action) in permissions {
                    perms.insert(
                        Value::String(name.clone()),
                        Value::String(permission_action(*action).into()),
                    );
                }
                inner.insert(Value::String("permissions".into()), Value::Mapping(perms));
            }
            if let Some(ms) = wait_for_interactive_ms {
                inner.insert(
                    Value::String("waitForInteractiveMs".into()),
                    Value::Number(Number::from(*ms)),
                );
            }
            Ok(single("launchApp", Value::Mapping(inner)))
        }
        other => Err(EmitError::Unsupported {
            verb: step_verb(other),
        }),
    }
}

fn single(verb: &str, value: Value) -> Value {
    let mut m = Mapping::new();
    m.insert(Value::String(verb.into()), value);
    Value::Mapping(m)
}

fn xy(p: (f64, f64)) -> Value {
    let mut m = Mapping::new();
    m.insert(Value::String("x".into()), Value::Number(Number::from(p.0)));
    m.insert(Value::String("y".into()), Value::Number(Number::from(p.1)));
    Value::Mapping(m)
}

fn permission_action(action: crate::MaestroPermissionAction) -> &'static str {
    match action {
        crate::MaestroPermissionAction::Allow => "allow",
        crate::MaestroPermissionAction::Deny => "deny",
        crate::MaestroPermissionAction::Unset => "unset",
    }
}

fn is_default_modifiers(m: &Modifiers) -> bool {
    m.near.is_none()
        && m.below.is_none()
        && m.above.is_none()
        && m.left_of.is_none()
        && m.right_of.is_none()
        && m.inside.is_none()
        && m.ancestor.is_none()
        && m.nth.is_none()
        && m.first.is_none()
        && m.last.is_none()
}

/// Selector → maestro yaml node the parser's selector arms read back.
/// Only the base forms with no modifiers and no lossy pattern survive a
/// faithful round-trip: `id` / `label` maps, and a bare literal text
/// string. A regex pattern (or a literal carrying `|`, which the parser
/// re-reads as a regex), any modifier, or any richer selector form is
/// refused.
fn selector_to_maestro(selector: &Selector) -> Result<Value, EmitError> {
    match selector {
        Selector::Id { id, modifiers } if is_default_modifiers(modifiers) => {
            let mut m = Mapping::new();
            m.insert(Value::String("id".into()), Value::String(id.clone()));
            Ok(Value::Mapping(m))
        }
        Selector::Label { label, modifiers } if is_default_modifiers(modifiers) => {
            let mut m = Mapping::new();
            m.insert(Value::String("label".into()), Value::String(label.clone()));
            Ok(Value::Mapping(m))
        }
        Selector::Text { text, modifiers } if is_default_modifiers(modifiers) => match text {
            Pattern::Text(s) if !s.contains('|') => Ok(Value::String(s.clone())),
            Pattern::Text(_) => Err(EmitError::Unsupported { verb: "text|regex" }),
            Pattern::Regex { .. } => Err(EmitError::Unsupported { verb: "text-regex" }),
        },
        Selector::Id { .. } | Selector::Label { .. } | Selector::Text { .. } => {
            Err(EmitError::Unsupported {
                verb: "selector-modifiers",
            })
        }
        Selector::Role { .. } => Err(EmitError::Unsupported { verb: "role" }),
        Selector::Focused { .. } => Err(EmitError::Unsupported { verb: "focused" }),
        Selector::Anchor { .. } => Err(EmitError::Unsupported { verb: "anchor" }),
        Selector::LocalizedText { .. } => Err(EmitError::Unsupported {
            verb: "localizedText",
        }),
        Selector::OcrText { .. } => Err(EmitError::Unsupported { verb: "ocrText" }),
        Selector::AnchorRelative { .. } => Err(EmitError::Unsupported {
            verb: "anchorRelative",
        }),
        Selector::Point { .. } => Err(EmitError::Unsupported { verb: "point" }),
        Selector::Fallback { .. } => Err(EmitError::Unsupported { verb: "fallback" }),
    }
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
        Step::ClearAppData { .. } => "clearAppData",
        Step::ResetAppData { .. } => "resetAppData",
        Step::ClearUserDefaults { .. } => "clearUserDefaults",
        Step::OpenLink(_) => "openLink",
        Step::StopApp => "stopApp",
        Step::Scroll => "scroll",
        Step::HideKeyboard => "hideKeyboard",
        Step::AssertNotVisible { .. } => "assertNotVisible",
        Step::KillApp { .. } => "killApp",
        Step::ClearState { .. } => "clearState",
        Step::ClearKeychain => "clearKeychain",
        Step::TakeScreenshot { .. } => "takeScreenshot",
        Step::SetClipboard(_) => "setClipboard",
        Step::PasteText { .. } => "pasteText",
        Step::CopyTextFrom { .. } => "copyTextFrom",
        Step::DoubleTapOn { .. } => "doubleTapOn",
        Step::RepeatTap { .. } => "repeatTap",
        Step::LongPressOn { .. } => "longPressOn",
        Step::AssertTrue { .. } => "assertTrue",
        Step::Repeat { .. } => "repeat",
        Step::Retry { .. } => "retry",
        Step::RunScript { .. } => "runScript",
        Step::EvalScript { .. } => "evalScript",
        Step::SetLocation { .. } => "setLocation",
        Step::Travel { .. } => "travel",
        Step::SetPermissions { .. } => "setPermissions",
        Step::AddMedia { .. } => "addMedia",
        Step::SetOrientation { .. } => "setOrientation",
        Step::StartRecording { .. } => "startRecording",
        Step::StopRecording => "stopRecording",
        Step::AssertScreenshot { .. } => "assertScreenshot",
        Step::AssertCondition { .. } => "assertCondition",
        Step::ExtractWithAI { .. } => "extractWithAI",
        Step::ExpectSignal { .. } => "expectSignal",
        Step::ExpectSignals { .. } => "expectSignals",
        Step::ExpectLogClean => "expectLogClean",
        Step::Fixture { .. } => "fixture",
    }
}
