# Migrating to smix 19.0.0

19.0.0 is a major release because two public Rust traits gained a required
method. Flows, the CLI and the SDKs need no change to keep working.

If you only write YAML or drive smix from the CLI, nothing changes:
`inputText`'s `method: setText` and `smix fill --set-text` are new, and
nothing that worked before answers differently. **Rust API** is for code
that implements smix's traits.

---

## Rust API

- `smix_driver::Driver` has a new required method
  `async fn set_text(&self, selector: &Selector, text: &str, include: Option<IncludeScope>, clear_first: bool) -> Result<(), ExpectationFailure>`.
  A `Driver` implemented outside smix must add it. There is no default: an
  implementation with no way to give a field its text without the keyboard
  should return an `ExpectationFailure` that says so, as the iOS driver
  does. It must not type the text instead.
- `smix_adapter_maestro::AppLike` has a new required method
  `async fn set_text(&self, selector: &Selector, text: &str) -> Result<(), ExpectationFailure>`.
  Test doubles that implement it add one that records or ignores the call.
- `smix_adapter_maestro::Step` has a new variant `SetText { selector, text }`
  (`selector: None` is the focused field). `Step` was already
  `#[non_exhaustive]`; a `match` with a wildcard arm keeps compiling.
- New in `smix_runner_client`: `HttpRunnerClient::set_text` and `set_text_in`.
- New in `smix_sdk`: `App::set_text`.

## Runners

`POST /input-text` takes `method: "type" | "setText"` and answers with the
`method` it used. A host at 19.0.0 asking an Android runner from an earlier
release to set text refuses the step: that runner ignores `method` and
types the text through the keyboard, which is what the caller asked to
avoid, and the missing `method` in its answer is how the host can tell.
Bring the runner up from 19.0.0 to set text. Typing is unchanged either
way.
