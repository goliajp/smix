# smix-ai-tier

The AI-assertion tier: hand a screenshot and a plain-language condition to a
local `claude` CLI, get a structured verdict back. It backs the
`assertCondition` and `extractWithAI` yaml verbs.

```rust,ignore
let verdict = smix_ai_tier::judge(&png, "a red error toast is visible", &cfg).await?;
if !verdict.pass {
    // verdict.reason explains what the judge saw
}
```

## This tier is fenced, and the fence is the point

smix senses the screen through the accessibility tree and Vision OCR. Those are
deterministic, and they are what every selector resolves through. This tier is
**not** part of that. It is an authoring and CI aid that sits beside the
resolver, never inside it:

- **Nothing that senses may depend on this crate.** Delete `smix-ai-tier` and
  the sense path still compiles and its tests still pass. That deletability is
  the fence, and it is enforced by a test rather than asserted in a comment.
- **Opt-in.** The verbs are inert unless a flow turns them on.
- **Marked non-deterministic.** A verdict is a judgement, not a measurement, and
  the output says so.
- **One provider.** The local `claude` CLI, invoked as a subprocess. There is no
  provider abstraction here and there will not be one.

## Failure is loud

A missing CLI, a timeout, or output that isn't the verdict we asked for are all
errors. None of them degrade into `pass: false` — that would report "your app is
broken" when the truth is "the judge never ran".
