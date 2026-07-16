# 10 — AI assertions

> Ask a model whether the screen looks right, when there is nothing on it worth
> selecting. Opt-in, non-deterministic, and deliberately kept out of the
> selector path.

## When to reach for this

Almost never, and that is the point.

`assertVisible: { id: "cart-total" }` is a measurement: the element is there or
it isn't, the same way every time. `assertCondition: "the cart total looks
right"` is a judgement — a model reads a screenshot and forms an opinion. Two
runs against the same screen may not agree.

So use the deterministic verbs whenever the screen gives you something to hold
onto — an id, a label, text, an OCR string. Reach for AI assertions when it
genuinely doesn't:

- **A visual claim with no element behind it.** "The chart is not clipped."
  "The avatar loaded rather than showing a broken image."
- **Porting a maestro flow** that already uses `assertWithAI` / `extractTextWithAI`,
  when you want it running before you rewrite it properly.
- **Reading a value off a screen** that renders it into an image or canvas,
  where OCR alone can't tell you which number is the total.

If you find yourself using it because a selector was hard to write, fix the
selector instead. A flaky judgement is worse than an honest failure.

## Turning it on

Off by default. A verdict costs a model call and isn't reproducible, so a flow
cannot reach for one by accident:

```bash
SMIX_ENABLE_AI_ASSERTIONS=1 smix run --device <udid> flow.yaml
```

Without it, a flow containing these verbs fails **at parse time** — before the
device is touched — rather than halfway through a run.

The judge is your local `claude` CLI, invoked as a subprocess. There is no
provider setting: smix does not ship a model abstraction, and it will not.

## `assertCondition`

```yaml
- assertCondition: "a red error toast is visible"
```

smix screenshots the device, hands the image and your condition to the CLI, and
reads back a verdict. When the condition doesn't hold, the step fails with the
judge's own reasoning:

```
FAIL [ASSERTION_FAILED]: [AI · non-deterministic] assertCondition did not hold:
a red error toast is visible — the judge saw: the screen shows a green success
banner, no error toast
  hint: this verdict is a judgement rather than a measurement; another run may
        answer differently
```

The `[AI · non-deterministic]` tag is on every AI-sourced failure, so a reader
skimming a CI log can tell a measurement from an opinion at a glance.

## `extractWithAI`

Reads named values off the screen into the output store, where the expression
engine can use them:

```yaml
- extractWithAI:
    into: order
    fields: ["total", "currency"]

- assertTrue: '${output["order.total"] != ""}'
```

**`into` is a key prefix, not a nested object.** The output store is flat, so
the fields above land at `order.total` and `order.currency`, and you read them
back with the bracket form. `${output.order.total}` does not parse.

`extractWithAI` is the only verb that writes the output store, which also makes
it the only way a `repeat.while` condition can change between iterations.

## When the judge doesn't answer

A missing CLI, a timeout, a non-zero exit, or a reply that isn't a verdict are
all **errors**, not `pass: false`:

```
FAIL [DRIVER_ERROR]: ai-tier: could not run the claude CLI at `claude`:
No such file or directory
  hint: install the claude CLI, or point claude_bin at it (currently `claude`)
```

This distinction matters more than it looks. Collapsing "the judge never ran"
into "the condition is false" would report a broken app when the truth is a
broken toolchain, and you would go debug the wrong thing.

## What this tier is not

It is not part of how smix sees the screen. Selectors resolve through the
accessibility tree and Vision OCR, and nothing in that path can reach this
crate — a check in CI asserts the sense path's dependency tree never touches it.
Delete the AI tier and every selector still resolves exactly as before.

That fence is why "smix uses AI" doesn't mean "smix is non-deterministic". The
deterministic core is the product; this is a tool sitting next to it.

## See also

- [02-yaml-reference.md](02-yaml-reference.md) — full grammar
- [03-selectors.md](03-selectors.md) — the deterministic way to find things
- [07-errors.md](07-errors.md) — reading a failure
