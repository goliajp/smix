# 03 — Selectors

> How to target an element on screen. smix supports 12 base selector forms + spatial / index modifiers. Pick by **specificity**: testid > role > text > spatial > coord.

## Specificity order (use the first one that works for your case)

```
strongest (production-ready, stable across rebuild / i18n / screen size)
  1. Id          — accessibility identifier (testTag) — PREFERRED
  2. Role        — semantic role + optional name pattern
  3. Label       — accessibility label (often = display text)
weaker (display text — can break under i18n / rename)
  4. Text        — literal or regex
  5. LocalizedText — per-locale text table
  6. OcrText     — Vision (iOS) / ML Kit (Android) — also handles non-accessibility text (PDFs, video labels, hardware tests)
spatial (when target lacks own id but a sibling does)
  7. Anchor      — anchor-only ("the button below Header")
  8. AnchorRelative — anchor + (dx, dy) normalized offset
worst (escape hatch — fragile, breaks on layout change)
  9. Focused     — current focus only
  10. Point      — direct viewport coordinate
  11. Fallback   — chain of any above; first hit wins
```

**Rule of thumb**: a yaml that uses `Id:` is durable; one that uses `Point:` is brittle. Reach for spatial / OCR only when the upstream component genuinely lacks an accessibility identifier you can fix.

## The same selector in three places

A selector is one idea written three ways, and the three are easy to
confuse when you move between them in one session:

| where | how it is written |
|---|---|
| flow yaml | `tapOn:`<br>`    id: "submit-button"` |
| CLI | `smix tap "id:submit-button"` |
| MCP tool | `{"id": "submit-button"}` |

The CLI's form is `field:value`, one colon, no spaces — `id=` is not
accepted, and the error says so with the right form in it. Every field
name is the same across all three; only the punctuation differs.

## Naming two things at once

A selector map may carry more than one base key. It means **and** — the
one element that satisfies both:

```yaml
# the element whose id is home-counter-label AND whose text is "1"
- assertVisible:
    id: "home-counter-label"
    text: "1"
```

The more specific key becomes the form (`id` > `label` > `text`, the
same order as the specificity list above); the rest narrow it. Any verb
that takes a selector reads it the same way.

**What this is not.** The spatial keys (`near`, `below`, `inside`, …)
constrain a candidate by *another* node's geometry, and `ancestor`
constrains it by its parent chain. This constrains the candidate
itself.

`role` + `name` is one form spelled with two keys, not a conjunction —
`name` has only ever been role's optional qualifier.

> Before 3.0 the second key was read by nobody: `{ id: X, text: Y }`
> parsed to `Text { Y }` and the id was dropped, so it matched any
> element reading Y. The form was already written in these guides, and
> in flows; what it meant is what it says now.

## The 12 selector forms

### 1. Id (preferred, stable)

```yaml
- tapOn:
    id: "home-increment-btn"
```

- Matches `accessibilityIdentifier` (iOS) / `testTag` via `testTagsAsResourceId` (Android Compose).
- Strict equality on id string.
- Use this whenever you can add a testid to the app under test.

### 2. Text (literal or regex)

```yaml
# Literal
- tapOn:
    text: "Submit"

# Regex — say so. A plain string is matched literally unless it
# contains `|`, which is the one character that promotes it.
- tapOn:
    text: { regex: "Row #[0-9]+" }

# Common pattern: anchored ^...$
- tapOn:
    text: { regex: "^Help$" }   # exclude "Help Center" etc
```

- Matches displayed text (label, attributed string, button title, accessible label).
- Regex syntax = NSRegularExpression on iOS, java.util.regex.Pattern on Android.
- `flags` defaults to `"i"`; matching is case-insensitive either way.
- A bare string is **not** scanned for metacharacters. `Delete?` and
  `3.5` are ordinary labels, and treating them as patterns would
  silently widen what they match — the failure that does not announce
  itself. The one exception is `|`, which has meant alternation here
  since before the explicit form existed.

### 3. Label (accessibility label)

```yaml
- tapOn:
    label: "Settings"
```

- Strictly matches `accessibilityLabel` (iOS) or `contentDescription` (Android).
- Differs from Text: an icon button often has `text:""` but `label:"Settings"`.

### 4. Role (semantic)

```yaml
- tapOn:
    role: button
    name: "OK"     # optional name pattern (text or regex)

- assertVisible:
    role: heading
    name: "Welcome"
```

- Supported roles (docs-friendly lowercase and camelCase both accepted; wire is camelCase): button, link, textField, secureTextField, searchField, switch, toggle, checkBox, radio, image, staticText (accepts `heading` as an alias), tab, tabBar, navigationBar, cell, alert, dialog, slider, progressBar, picker, menu, menuItem, scrollView, segmentedControl, table, collectionView, webView, keyboard.
- iOS: derived from XCUIElement type + traits. Android: from AccessibilityNodeInfo class name + roleDescription.
- `role:` and its optional `name:` work anywhere a selector map does — `tapOn:`, `assertVisible:`, `extendedWaitUntil.visible:`, `scrollUntilVisible:`, etc.

### 5. Focused

```yaml
- inputText: "hello"      # the scalar form types into the focused field, appending

- pressKey: ENTER     # often used after focused: true to submit
```

- Targets whichever element currently has keyboard focus.
- No modifiers (no `nth`, no spatial — there's only one focused element).

### 6. Anchor (spatial — anchor-only)

```yaml
- tapOn:
    text: "Edit"
    below: { text: "Settings" }   # the Edit below the Settings row

# Pixel-offset form — anchor plus a dx/dy nudge, all three required:
- tapOn:
    anchored:
      anchor: { text: "Settings" }
      dx: 0.0
      dy: 40.0
```

- `anchor` finds a stable reference element by text or id.
- Then a spatial key selects what to tap relative to it.
- Spatial keys: `near` / `below` / `above` / `leftOf` / `rightOf` / `inside` / `ancestor`.

### 7. AnchorRelative (anchor + offset)

```yaml
- tapOn:
    anchorRelative:
      anchor: "Header bar"
      dx: 0.4         # +0.4 viewport width to the right
      dy: -0.05       # -0.05 viewport height above
```

- Useful for elements that are visually adjacent but lack any accessibility relationship.
- dx/dy are viewport-normalized (so layout-tolerant).

### 8. LocalizedText (per-locale tables)

```yaml
- tapOn:
    localizedText:
      en: "Submit"
      ja: "送信"
      es: "Enviar"
```

- Detects current locale at runtime (iOS Locale.current / Android LocalConfiguration).
- Picks the matching string from the table.
- Fails if current locale has no entry.

### 9. OcrText (Vision / ML Kit)

```yaml
- tapOn:
    ocrText: "Done"

- tapOn:
    ocrText: "Confirm"
    locales: ["en", "ja"]         # optional recognition language list
    recognition_level: accurate   # iOS Vision (fast | accurate); ignored on Android
```

- iOS: Apple Vision VNRecognizeTextRequest.
- Android: ML Kit Latin TextRecognition.
- Slow (~150-300ms per call) — use only when target has no testid AND text-based selectors miss (e.g., text rendered as image, PDF viewer, video frame label).

### 10. Point (direct coordinate)

```yaml
- tapOn:
    point: "50%,80%"        # X%,Y% of viewport
```

- nx/ny are normalized [0, 1] of viewport.
- Brittle: fails any time layout shifts.
- Use only when the target has no other identifier AND you control the layout (e.g., dev menu coord targets, OCR-derived coord).

### 11. Fallback (chain)

```yaml
- tapOn:
    fallback:
      - id: "home-cta-btn"              # try first
      - text: "Get Started"             # if id absent
      - ocrText: "Get Started"          # if text absent (rendered as image)
      - point: "50%,75%"                # last resort
```

- Iterates entries in order; uses the first that resolves. Order is a
  promise, not a tiebreak: `[id, text]` prefers the id even when both
  match, so a chain keeps picking the same element as an app's copy
  changes.
- Works in any selector position, in every verb that takes a selector —
  `assertVisible`, `tapOn`, `extendedWaitUntil`, `fill`, and the rest —
  and on both platforms.
- Where a verb returns every match rather than one, a chain gives you
  the first layer that matched, not the union of the layers.
- A layer whose pattern cannot compile is skipped; the layers after it
  still get their turn.
- Handy for production code where the same logical button might have testid in dev builds but not release builds.
- `ocrText`, `localizedText` and `anchorRelative` are read above the
  tree resolver, and only some verbs do that — `tapOn`,
  `extendedWaitUntil` and `scrollUntilVisible`. Elsewhere they match
  nothing, so **`assertNotVisible` and `waitForNotVisible` refuse them
  rather than passing**: an absence check that never looked is not
  evidence of absence. `assertVisible` still fails, and says which part
  went unchecked. This applies to a chain containing one of them too.

### 12. (Implicit shortcut) String

```yaml
- tapOn: "Submit"     # equivalent to: tapOn: { text: "Submit" }
```

- Convenience shorthand for `text:` only.

## Modifiers

### Spatial (works on Text / Id / Label / Role / Anchor selectors)

```yaml
- tapOn:
    text: "Cancel"
    near: "Login"            # closest visible "Cancel" to "Login" anchor

- tapOn:
    role: button
    below: "Header"          # button below the Header element

- tapOn:
    role: textfield
    rightOf: "Email"         # textfield to the right of "Email" label

- tapOn:
    id: "list-row-content"
    inside: "modal-list"     # row that is descendant of modal-list

- tapOn:
    role: button
    ancestor: "modal-overlay"  # button anywhere in the modal subtree
```

### Index (nth / first / last)

```yaml
- tapOn:
    text: "Item.*"
    nth: 2          # 0-indexed: 3rd match

- tapOn:
    text: "Card.*"
    first: true     # equivalent to nth: 0

- tapOn:
    text: "Page.*"
    last: true      # last in document order
```

### Combining modifiers

Spatial + index = picky:

```yaml
- tapOn:
    text: "Edit"
    inside: "modal"
    nth: 1            # 2nd "Edit" within modal subtree
```

## Selector resolution diagnostics

If a selector matches zero elements, the failure message includes:
- The selector you used (full canonicalized form)
- `visibleElements:` partial dump of what was on screen
- `suggestions:` text strings of nearby elements that *could* have been what you meant

Read the suggestions block carefully — it's tuned to surface i18n drift and missing testid issues.

```
ELEMENT_NOT_FOUND: tap_by_id: element not found — id="home-incremnt-btn"
  visibleElements:
    - id="home-counter-label" text="0"
    - id="home-increment-btn" text="+1"   ← typo in your selector!
    - id="home-reset-btn" text="Reset"
  suggestions:
    - "home-increment-btn" (id, exact match to a sibling)
```

## Common pitfalls

- **Compose modal hosts in separate window.** `testTagsAsResourceId` on a root screen does NOT propagate into BottomSheet / AlertDialog. Each modal content + dialog button must add its own `.semantics { testTagsAsResourceId = true }`. See [08-cookbook.md](08-cookbook.md) §modal-testtags.
- **AlertDialog buttons need PER-BUTTON testTagsAsResourceId**, not just on content level.
- **LazyRow / LazyColumn lazy-render**: off-screen items don't generate AccessibilityNodeInfo. Scroll first (`scrollUntilVisible` or `adb shell input swipe`) then select. Tree dump only shows currently-visible items.
- **iOS Map (MapKit) and Camera (AVPreviewView) sub-widgets** are heavy native views — XCUI sees them as opaque `Other` elements with no children. Use accessibilityIdentifier on the wrapping View, not internal sub-widgets.
- **Vision OCR returns frame in (nx, ny, w, h) tuple**, not (nx_center, ny_center) — adjust if you compute tap point from OCR.

## See also

- [02-yaml-reference.md](02-yaml-reference.md) §selectors inside steps
- [06-fixtures.md](06-fixtures.md) — a testTag layout example you can mirror in your own app
