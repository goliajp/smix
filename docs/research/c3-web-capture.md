# C3 — web capture obtainability (v2.10 recorder, Playwright bridge)

Can Playwright capture a web interaction stream (click / input) reconstructible
into `smix-authoring-ir::IRAction`, mapping DOM targets to `smix_selector::Selector`
— without a physical device (§9#1: a driver-layer DOM bridge, not a real device)?

## Reference: what the native legs capture

iOS `EventRecorder` swizzles the AX-notification stream; Android
(`RecordMapper`, v2.10-C2, device-verified) listens on
`UiAutomation.setOnAccessibilityEventListener`. Both are **passive OS event
streams**, mapped to `IRAction{kind,selector,timestampMs}` with the element's
stable id (`accessibilityIdentifier` / `viewIdResourceName`). The web leg must
find the same shape: a passive capture + a stable selector.

## Falsification rubric

- **Capture OBTAINABLE**: a documented Playwright API passively delivers the
  user's `click` / `input` events to Node (not a generated script to parse, not
  a poll). **NOT-OBTAINABLE**: only codegen-script output exists, or `page.on`
  carries no user input events.
- **Selector OBTAINABLE**: a DOM target yields a stable `Selector` variant.
  PARTIAL if only some targets do; the winning main path must be as stable as
  the native ids.
- Overall **PARTIAL** if capture is obtainable but a class of selectors/actions
  is not cleanly reconstructible (recorded gap, not faked) — the honest verdict
  when the mechanism works but the web selector vocabulary is narrower than the
  native a11y-id one.

## Evidence

### Capture — OBTAINABLE via injection (addInitScript + exposeBinding)

- **`page.on(...)` carries no user input** (documented: console / request /
  response / dialog / popup / frame events — not user click/keydown). Ruled out.
- **`playwright codegen` emits a generated script** (`page.getByRole('button')
  .click()`), a source artifact, not a consumable event stream — parsing it is
  brittle. Ruled out as the capture axis.
- **Injection is the documented, stable equivalent**: `page.addInitScript(fn)`
  runs `fn` before any page script on every navigation; an injected
  capture-phase `addEventListener('click'|'input', …, true)` observes user
  events; `page.exposeBinding(name, cb)` installs a `window` function (surviving
  navigation) that forwards each captured event to Node. This is the web analogue
  of the iOS swizzle / Android listener — a passive in-page tap on the DOM event
  stream, not a script parse. **OBTAINABLE** for click -> Tap, input ->
  Fill/Clear (with keystroke debounce, mirroring Android RecordMapper's coalesce).

### Selector — PARTIAL (`data-testid` main path, role secondary)

The web has no a11y-id system as universal as `accessibilityIdentifier` /
`viewIdResourceName`. Mapping priority, most-stable first:
- **`data-testid` -> `Selector::Id`** — the author-declared stable test id
  (Playwright's `getByTestId` default attribute); the honest web equivalent of
  the native stable ids. Main path.
- **ARIA `role` (if in smix's vocabulary) -> `Selector::Role`** — PARTIAL:
  `button/link/checkbox/radio/switch/tab/menu/menuitem/dialog/alert/slider/
  image/table` map; `textbox` (!= smix's iOS-centric `textField`), `combobox`,
  `listbox`, `option`, `spinbutton`, `searchbox` (!= `searchField`) have no
  clean `Selector` and are a gap, not faked.
- **visible `textContent` -> `Selector::Text`** for click targets (button/link).
- No stable target -> dropped + counted (mirrors Android null-viewId). The raw
  DOM `id` attribute is NOT a main path (structural, not a test-semantic id).

## VERDICT

VERDICT: PARTIAL — Playwright CAN passively capture click -> Tap and input ->
Fill/Clear via injection (`addInitScript` + `exposeBinding`), the documented,
stable web equivalent of the native AX streams, and `data-testid -> Selector::Id`
is a stable main selector path (the honest web peer of the native ids). It is
PARTIAL, not OBTAINABLE, because smix's iOS-centric `Role` vocabulary does not
cover several ARIA roles (`textbox`/`combobox`/`searchbox`/…), so role-based and
untagged targets without a `data-testid` or mappable role are a recorded gap,
not fabricated. The web leg emits the same IRAction JSON as the Android leg; the
record->generate glue stays cross-platform (C4).
