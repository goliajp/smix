//! What a reader does next, for each refusal the runner names.

/// The hint for a refusal the runner gave as `kind`, having seen `saw`.
pub(crate) fn hint(kind: &str, saw: &str) -> String {
    match kind {
        "keyboard_did_not_close" if saw.contains("; return key: ") => {
            "the field gives the keyboard up only through its return key, named \
             in the `saw` above, and hideKeyboard does not press a return key \
             named for an action: it submits the field. If submitting is what \
             the flow wants, `pressKey: enter` presses it."
                .to_string()
        }
        "keyboard_did_not_close" => "the keyboard was there and every dismiss \
             strategy ran without closing it — the `saw` above names what was \
             tried and what still holds focus. Tapping the next control often \
             works when the field will not give focus up; `hideKeyboard` is \
             already a no-op when no keyboard is present, so a guard around it \
             is not what this needs."
            .to_string(),
        "keyboard_state_unknown" => "the runner raised while looking, so nothing \
             was established about the keyboard — this is not evidence it is \
             still up. Retry the step; if it repeats, the runner is the thing \
             to look at, not the screen."
            .to_string(),
        "no_such_button" => "this device has no such button — the `saw` above \
             says why and what to do instead. It is refused rather than skipped so \
             a flow that needs the press cannot pass without it."
            .to_string(),
        other => format!(
            "the runner refused with `{other}` — the `saw` above is what it \
             observed"
        ),
    }
}
