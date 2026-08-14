# 12 — Authoring flows from an LLM

This guide is for a language model writing and debugging smix flows — the
process, not the syntax. The syntax lives in [03-selectors](03-selectors.md) and
[04-actions](04-actions.md); the recipes for common screens live in
[08-cookbook](08-cookbook.md). What follows is how to *approach* the task so the
loop converges instead of guessing.

## The one idea

smix is deterministic sense → act, and every failure is written to be read by
you, not a human. A step does not just pass or fail — a failure carries the
`visibleElements` it saw, a `code`, and `suggestions`. **Authoring is a loop:
run, read what it saw, adjust the selector, run again.** You are not expected to
know the tree in advance; you are expected to read it back.

## Before running: `--dry-run`

Parse-only, no simulator, no runner. It validates every step (and every
`runFlow:` include) and reports `parse OK/FAIL` per file. Run it first — a
parse error costs a second here instead of a device round trip.

```bash
smix run flow.yaml --device <DEVICE> --dry-run
```

## Picking a selector

Order of preference, most to least stable:

1. **`id`** — an accessibility identifier. Stable across copy changes and
   locales; nothing else is. Prefer it whenever the element has one.
2. **`label`** — the accessibility label. Stable-ish, but localised.
3. **`text`** — visible text. Fine for buttons and headings; breaks when the
   copy or language changes.
4. **`role` + `name`** — when several elements share text but differ in kind.
5. **`ocrText`** — the pixels, read by OCR. The escape hatch for elements the
   accessibility tree does not expose at all (a React Native Fabric tree that
   dropped its identifiers, a canvas-drawn control).

When the accessibility tree is unreliable — iOS 26.5 + RN Fabric is the case
this project keeps hitting — combine tiers in a `fallback` so the tap tries the
tree first and OCR second:

```yaml
- tapOn:
    fallback:
      - id: sign-in-btn
      - text: Sign In
      - ocrText: Sign In
```

## Reading a failure

A timeout or a not-found does not mean "the app is broken". It means smix
looked and tells you what it saw. Read `visibleElements` — the elements it
*did* find are the menu you pick your next selector from. If your `text: Login`
missed but `visibleElements` shows `staticText name="Log in"`, the copy differs
by a space and a case; fix the selector, not the app.

`suggestions` names the likely fix. `code` sorts the failure:

- `ELEMENT_NOT_FOUND` — nothing matched. Widen: try `fallback`, or read
  `visibleElements` and pick what is actually there.
- `NOT_VISIBLE` — matched but off-screen or occluded. Scroll to it first.
- `AMBIGUOUS` — several matched. Add `role`, `nth`, or a spatial modifier
  (`below:`, `rightOf:`) to disambiguate.
- `TAP_MISSED` — the tap landed outside the element it aimed at (the screen
  moved between the tree fetch and the tap). Wait for the screen to settle
  (`extendedWaitUntil`) before tapping.

Full code list: [07-errors](07-errors.md).

## Reading the tree directly

When you cannot infer a selector from a failure, dump the tree and pick from it:

```bash
smix tree --device <DEVICE>
```

Every node carries its `identifier`, `label`, and `role`. A node with an empty
identifier *and* empty label whose `elementTypeRaw` is not 1 is a React Native
accessibility-bridge drop — that element is unreachable by the tree, so reach it
by `ocrText` or by a nearby labelled anchor (`below:`, `rightOf:`).

## Waiting instead of sleeping

There is no bare `sleep`. Wait on the thing you actually need:

```yaml
- extendedWaitUntil:
    visible: Welcome
    timeout: 10000
```

For an element that appears only after an animation or a network round trip,
`extendedWaitUntil` with a `fallback` selector polls the tree and OCR each
iteration rather than guessing a fixed delay.

## Confirming a tap did something

A green `tapOn` means "the point aimed at was inside the element named" — its
success output says so in those terms (`aimed inside: …`). It does **not** mean
the app reacted, and it does not mean the app is where an unchanged screen came
from: this line is judged in the snapshot's coordinate space, and a mismatch
between that space and the one the touch is delivered in produces exactly this
output with nothing moving. Assert the *consequence*, not the tap:

```yaml
- tapOn: { id: open-menu }
- assertVisible: Settings
```

## When the tree cannot decide — the AI-assertion tier

For a check the deterministic layer cannot make ("does this look like an error
state"), the fenced AI tier takes a screenshot to a local `claude` CLI and
returns a structured verdict. It is opt-in and marked non-deterministic, and it
sits beside the resolver, never inside it — see [10-ai-assertions](10-ai-assertions.md).
Reach for it only when a selector genuinely cannot express the check; a
deterministic assertion is always preferable.

## The loop, in one place

1. `--dry-run` to catch parse errors with no device.
2. Run. On failure, read `visibleElements` + `suggestions` + `code`.
3. Adjust the selector from what was actually seen — do not re-guess blindly.
4. Stuck? `smix tree` and pick from the real nodes.
5. Assert consequences, not actions.
6. Only reach for the AI tier when no selector can express the check.
