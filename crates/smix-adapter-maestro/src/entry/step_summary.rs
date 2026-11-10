use crate::Step;

/// A step's one-line description, richer than its verb; `run_steps` writes
/// it as each step starts, the debug record keys off the bare verb.
pub(crate) fn summarize_step(step: &Step) -> String {
    match step {
        Step::Optional(inner) => format!("{} (optional)", summarize_step(inner)),
        Step::UntilEffect {
            action,
            until,
            until_visible,
            within_ms,
            retries,
        } => format!(
            "{} until {} {} within {within_ms} ms, retries {retries}",
            summarize_step(action),
            if *until_visible {
                "visible"
            } else {
                "notVisible"
            },
            smix_sdk::describe_selector(until)
        ),
        Step::RepeatTap { times, .. } => format!("repeatTap x{times}"),
        Step::Drag {
            from, to, hold_ms, ..
        } => format!(
            "drag {} → {} (hold {hold_ms} ms)",
            smix_sdk::describe_selector(from),
            smix_sdk::describe_selector(to)
        ),
        Step::Pinch { spans, pan, .. } => {
            let (a, b, unit) = match spans {
                smix_input::PinchSpans::Scales { from, to } => (*from, *to, "x"),
                smix_input::PinchSpans::Spans { from, to } => (*from, *to, " span"),
            };
            let drag = if pan.is_some() { " then drag" } else { "" };
            format!("pinch {a}{unit} → {b}{unit}{drag}")
        }
        Step::SwipeOver { from, to, .. } => format!(
            "swipe over element {:.0}%,{:.0}% → {:.0}%,{:.0}%",
            from.0 * 100.0,
            from.1 * 100.0,
            to.0 * 100.0,
            to.1 * 100.0
        ),
        Step::TapOn { optional, .. } => {
            if *optional {
                "tapOn (optional)".into()
            } else {
                "tapOn".into()
            }
        }
        Step::TapAtPoint { nx, ny } => format!("tapAtPoint ({nx:.3}, {ny:.3})"),
        Step::WebViewEval { js, .. } => {
            let preview = if js.len() > 40 { &js[..40] } else { js };
            format!("webview_eval `{preview}`")
        }
        Step::WaitForAnimationToEnd { .. } => "waitForAnimationToEnd".into(),
        Step::ExtendedWaitUntil { timeout_ms, .. } => {
            format!("extendedWaitUntil (timeout {timeout_ms}ms)")
        }
        Step::AssertVisible { .. } => "assertVisible".into(),
        Step::InputText(s) => format!("inputText ({} chars)", s.chars().count()),
        Step::InputSecret { selector, env, .. } => match selector {
            Some(sel) => format!(
                "inputText secret {env} into {}",
                smix_sdk::describe_selector(sel)
            ),
            None => format!("inputText secret {env}"),
        },
        Step::InputTextInto { selector, text } => {
            format!(
                "inputText into {selector:?} ({} chars)",
                text.chars().count()
            )
        }
        Step::SetText { selector, text } => match selector {
            Some(selector) => format!(
                "inputText into {selector:?}, set ({} chars)",
                text.chars().count()
            ),
            None => format!("inputText, set ({} chars)", text.chars().count()),
        },
        Step::PressKey(k) => format!("pressKey {k}"),
        Step::PressKeyCombo(c) => format!("pressKey {c}"),
        Step::RunFlow(p) => format!("runFlow {p}"),
        Step::RunFlowConditional {
            file, when, opts, ..
        } => format!(
            "runFlow {file}{}{}",
            if when.is_some() { " (conditional)" } else { "" },
            block_label(opts)
        ),
        Step::RunFlowInline {
            steps, when, opts, ..
        } => {
            let cond = if when.is_some() { " (conditional)" } else { "" };
            format!(
                "runFlow inline ({} cmds){cond}{}",
                steps.len(),
                block_label(opts)
            )
        }
        Step::ScrollUntilVisible {
            direction, opts, ..
        } => {
            format!("scrollUntilVisible ({direction}){}", block_label(opts))
        }
        Step::EraseText(n) => format!("eraseText ({n} chars)"),
        Step::Swipe { from, to } => format!("swipe {from:?} -> {to:?}"),
        Step::LaunchApp { app_id, .. } => format!("launchApp {app_id}"),
        Step::OpenLink(url) => format!("openLink {url}"),
        Step::SendPush(p) => p.summary(),
        Step::StopApp => "stopApp".into(),
        Step::ClearAppData { .. } => "clearAppData".into(),
        Step::ResetAppData { .. } => "resetAppData".into(),
        Step::ClearUserDefaults { keys, .. } => format!(
            "clearUserDefaults ({} key{})",
            keys.len(),
            if keys.len() == 1 { "" } else { "s" }
        ),
        Step::AssertCondition { condition } => format!("assertCondition {condition:?} [AI]"),
        Step::ExtractWithAI { into, fields } => {
            format!(
                "extractWithAI → {into} ({} field{}) [AI]",
                fields.len(),
                if fields.len() == 1 { "" } else { "s" }
            )
        }
        Step::Scroll => "scroll".into(),
        Step::HideKeyboard => "hideKeyboard".into(),
        Step::HoverOn { selector } => {
            format!("hoverOn {}", smix_sdk::describe_selector(selector))
        }
        Step::HoverAway => "hoverAway".into(),
        Step::RightClickOn { selector } => {
            format!("rightClickOn {}", smix_sdk::describe_selector(selector))
        }
        Step::MouseWheel {
            selector,
            up,
            notches,
            ctrl,
        } => format!(
            "mouseWheel {} {} {notches}{}",
            smix_sdk::describe_selector(selector),
            if *up { "up" } else { "down" },
            if *ctrl { " ctrl" } else { "" }
        ),
        Step::MouseDrag { from, to } => match to {
            crate::MouseDragTo::Element(sel) => format!(
                "mouseDrag {} to {}",
                smix_sdk::describe_selector(from),
                smix_sdk::describe_selector(sel)
            ),
            crate::MouseDragTo::ByDp((x, y)) => format!(
                "mouseDrag {} by {x},{y} dp",
                smix_sdk::describe_selector(from)
            ),
        },
        Step::SetWindowSize { size } => format!("setWindowSize {size}"),
        Step::AssertNotVisible { .. } => "assertNotVisible".into(),
        Step::Back(_) => "back".to_string(),
        Step::KillApp { app_id } => match app_id {
            Some(id) => format!("killApp {id}"),
            None => "killApp (current)".to_string(),
        },
        Step::ClearState { app_id } => match app_id {
            Some(id) => format!("clearState {id}"),
            None => "clearState (current)".to_string(),
        },
        Step::ClearKeychain => "clearKeychain".into(),
        Step::TakeScreenshot {
            path,
            annotations,
            crop_on,
            window,
        } => {
            let mut s = match path {
                Some(p) => format!("takeScreenshot {p}"),
                None => "takeScreenshot".into(),
            };
            if !annotations.is_empty() {
                s.push_str(&format!(" +{} annotate", annotations.len()));
            }
            if let Some(sel) = crop_on {
                s.push_str(&format!(
                    " cropOn={}",
                    smix_selector::describe_selector(sel)
                ));
            }
            if *window {
                s.push_str(" window");
            }
            s
        }
        Step::SetClipboard(s) => format!("setClipboard ({} chars)", s.chars().count()),
        Step::PasteText { text } => match text {
            Some(s) => format!("pasteText ({} chars)", s.chars().count()),
            None => "pasteText (from clipboard)".into(),
        },
        Step::CopyTextFrom { .. } => "copyTextFrom".into(),
        Step::RememberBounds { .. } => "rememberBounds".into(),
        Step::AssertBoundsUnchanged { .. } => "assertBoundsUnchanged".into(),
        Step::NeverVisible { .. } => "neverVisible".into(),
        Step::DoubleTapOn { .. } => "doubleTapOn".into(),
        Step::LongPressOn { duration_ms, .. } => format!("longPressOn ({duration_ms}ms)"),
        Step::AssertTrue { .. } => "assertTrue".into(),
        Step::Repeat {
            times,
            while_,
            while_expr,
            commands,
            opts,
        } => {
            // Both halves when both are there: "× 3 while condition" is
            // a different loop from either alone, and a summary that
            // named one would describe a loop the flow did not write.
            let how = [
                times.map(|n| format!("× {n}")),
                while_.as_ref().map(|_| "while condition".to_string()),
                while_expr.as_ref().map(|_| "while".to_string()),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
            format!(
                "repeat {how} ({} cmds){}",
                commands.len(),
                block_label(opts)
            )
        }
        Step::Retry {
            max_retries,
            commands,
        } => format!("retry (max {max_retries}, {} cmds)", commands.len()),
        Step::RunScript { .. } => "runScript".into(),
        Step::EvalScript { .. } => "evalScript".into(),
        Step::SetLocation {
            latitude,
            longitude,
        } => format!("setLocation ({latitude}, {longitude})"),
        Step::ClearLocation => "clearLocation".into(),
        Step::Travel { points, .. } => format!("travel ({} waypoints)", points.len()),
        Step::SetPermissions { permissions, .. } => {
            format!("setPermissions ({} entries)", permissions.len())
        }
        Step::AddMedia { paths } => format!("addMedia ({} paths)", paths.len()),
        Step::SetOrientation { orientation } => format!("setOrientation {orientation:?}"),
        Step::SetPosture { posture } => format!("setPosture {}", posture.name()),
        Step::RotateCrown { rotations, .. } => format!("rotateCrown {rotations}"),
        Step::StartRecording { path } => format!("startRecording {path}"),
        Step::StopRecording => "stopRecording".into(),
        Step::AssertScreenshot {
            path,
            threshold,
            mask,
            crop_on,
        } => {
            let mut s = format!("assertScreenshot {path}");
            match threshold {
                crate::ScreenshotThreshold::Hash(Some(c)) => s.push_str(&format!(" threshold={c}")),
                crate::ScreenshotThreshold::Hash(None) => {}
                crate::ScreenshotThreshold::Percentage(p) => {
                    s.push_str(&format!(" thresholdPercentage={p}"))
                }
            }
            if let Some(sel) = crop_on {
                s.push_str(&format!(
                    " cropOn={}",
                    smix_selector::describe_selector(sel)
                ));
            }
            if !mask.is_empty() {
                s.push_str(&format!(" mask={}regions", mask.len()));
            }
            s
        }
        Step::ExpectSignal {
            regex, timeout_ms, ..
        } => format!("expect.signal /{regex}/ timeoutMs={timeout_ms}"),
        Step::ExpectSignals {
            signals,
            order,
            timeout_ms,
            ..
        } => format!(
            "expect.signals ({} matcher{}, order={:?}) timeoutMs={timeout_ms}",
            signals.len(),
            if signals.len() == 1 { "" } else { "s" },
            order
        ),
        Step::ExpectLogClean => "expect.logClean".into(),
        Step::Fixture { id, timeout_ms } => match timeout_ms {
            Some(t) => format!("fixture {id} (timeoutMs={t})"),
            None => format!("fixture {id}"),
        },
    }
}

/// ` "label"` for a labelled block — maestro reports a labelled command
/// by its label.
fn block_label(opts: &crate::BlockOptions) -> String {
    opts.label
        .as_ref()
        .map(|l| format!(" \"{l}\""))
        .unwrap_or_default()
}
