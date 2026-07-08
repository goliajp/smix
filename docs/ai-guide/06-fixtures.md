# 06 — Test fixture layout (example)

> A reference layout for the test app you drive with smix — how to organize screens, testTags, and shared IDs so YAML flows stay maintainable across iOS and Android. Mirror this pattern in your own app; the exact IDs are yours to choose.

## What is a "fixture"?

A dedicated app (or a build variant of your production app) that exposes deterministic UI for driving via smix:

- **iOS**: SwiftUI (or UIKit) app, own bundle id (e.g. `com.example.app`).
- **Android**: Jetpack Compose (or Views) app, own package (e.g. `com.example.app`).

The key property: **testTag (Compose) ≡ accessibilityIdentifier (SwiftUI)**. The same string identifies the same role on both platforms, so a single YAML can drive both.

## testTag naming convention

Adopt a hierarchical, kebab-case pattern:

```
<screen>-<element>-<kind>

home-counter-label
home-increment-btn
home-reset-btn
form-email-input
form-submit-btn
modal-sheet-dismiss-btn
list-row-<N>
list-row-<N>-title
```

- `<screen>` — the screen area (home, form, list, modal, …)
- `<element>` — the semantic element within the screen
- `<kind>` — role suffix (`-btn`, `-label`, `-input`, `-screen`, `-row`, `-check`, …)

Consistency is worth more than terseness. A YAML that says `tapOn: { id: "form-submit-btn" }` reads unambiguously; `tapOn: { id: "fs" }` does not.

## Example screen layout

```
RootScreen (tab bar)
├── Home         ← counter + increment + reset + alert
├── Form         ← 3 text fields + submit
├── List         ← N rows alternating bg
├── Modal        ← multiple modal types (sheet / alert / action / full)
├── Perm         ← permission requests
├── Loc          ← location pull
├── Orient       ← orientation read
├── Kbd          ← keyboard focus / submit / hide
├── Clip         ← clipboard copy/paste
├── Deeplink     ← deep-link URL handler
├── Push         ← local notification trigger
├── Localized    ← untagged buttons with locale-dependent labels
├── OCR          ← untagged labels at varied sizes
├── Anchor       ← icon w/ clickable + last-tapped state
├── WebView      ← WebView w/ inline HTML form
├── DeepNav      ← N-level nested nav stack
├── Stacked      ← modal stack (sheet ⊃ alert ⊃ sheet)
├── Heavy        ← virtualized list (10k+ rows)
├── Map          ← MapKit / osmdroid
├── Cam          ← CameraX / AVCaptureSession
└── Wiz          ← multi-step wizard
```

## Tab navigation testTags

Every tab should be reachable via:

```yaml
- tapOn: { id: "tab-<area>" }
# Where <area> is the lowercase area name (home, form, list, modal, ...)
```

## Per-screen testTag tables (representative)

### HomeScreen

| testTag | Description |
|---|---|
| `screen-home` | Container |
| `home-counter-label` | Counter value display |
| `home-increment-btn` | +1 button |
| `home-reset-btn` | Reset button |
| `home-show-alert-btn` | Open alert |
| `home-alert-ok-btn` | Alert OK button (in dialog) |
| `home-alert-cancel-btn` | Alert cancel |

### FormScreen

| testTag | Description |
|---|---|
| `screen-form` | Container |
| `form-name-input` | Name field |
| `form-email-input` | Email field |
| `form-password-input` | Password field |
| `form-submit-btn` | Submit |
| `form-submitted-label` | Result display after submit |

### ListScreen

| testTag | Description |
|---|---|
| `screen-list` | Container |
| `list-row-<N>` | Each row (N = 0..N-1) |

### ModalScreen

| testTag | Description |
|---|---|
| `screen-modal` | Container |
| `modal-open-sheet-btn` | Open BottomSheet / .sheet |
| `modal-open-alert-btn` | Open AlertDialog / .alert |
| `modal-open-actionsheet-btn` | Open action sheet |
| `modal-open-fullscreen-btn` | Open fullScreenCover |
| `modal-sheet-dismiss-btn` | Inside sheet — dismiss |
| `modal-alert-ok-btn` | Inside alert — OK |
| `modal-action-a-btn` | Inside action sheet — option A |
| `modal-action-cancel-btn` | Inside action sheet — cancel |
| `modal-fullscreen-dismiss-btn` | Inside full screen — dismiss |

### DeepNavScreen (multi-level nav)

| testTag | Description |
|---|---|
| `screen-deepnav` | Container |
| `deepnav-l1-screen` | Level 1 (Categories) |
| `deepnav-l1-<category>-btn` | Per-category button |
| `deepnav-l2-screen` | L2 (Subcategories) |
| `deepnav-l2-<sub>-btn` | Per-subcategory button |
| `deepnav-l3-screen` | L3 (Items) |
| `deepnav-l3-item<N>-btn` | Item N |
| `deepnav-l4-screen` | L4 (Detail) |
| `deepnav-l4-edit-btn` | Edit |
| `deepnav-l5-screen` | L5 (Edit form) |
| `deepnav-l5-text-input` | Text field |
| `deepnav-l5-save-btn` | Save → pops back with state |

### Heavy list (virtualized rows)

| testTag | Description |
|---|---|
| `screen-heavylist` | Container |
| `heavylist-list` | LazyColumn / List |
| `heavylist-first-visible-label` | "first visible: N" |
| `heavylist-last-tapped-label` | "last tapped: row#N" |
| `heavylist-jump-input` | Number input |
| `heavylist-jump-btn` | Scroll-to-row |
| `heavylist-row-<N>` | Row container (lazy — only visible rows enumerate) |
| `heavylist-row-<N>-title` | Title |
| `heavylist-row-<N>-btn` | Tap button |
| `heavylist-row-<N>-check` | Checkbox |

### WebView

| testTag | Description |
|---|---|
| `screen-webview` | Container |
| `webview-title-label` | Title text |
| `webview-container` | WebView host |
| (HTML inside: `<button data-testid="webview-submit-btn">` and `<input id="user-input">`) |

For flows using `webViewEval`, see [04-actions.md](04-actions.md) §WebView JS bridge.

### Wizard (multi-step onboarding)

| testTag | Description |
|---|---|
| `screen-wizard` | Container |
| `wizard-step-label` | "step 2 / 3" |
| `wizard-progress-bar` | LinearProgressIndicator |
| `wizard-step1` / `step2` / `step3` | Per-step subcontainer |
| `wizard-radio-<option>` | Step 1 radios |
| `wizard-name-input` | Step 2 |
| `wizard-company-input` | Step 2 (conditional) |
| `wizard-email-input` | Step 3 |
| `wizard-summary-label` | Step 3 summary |
| `wizard-back-btn` | Back |
| `wizard-next-btn` | Next |
| `wizard-submit-btn` | Submit |
| `wizard-validation-label` | Error message |
| `wizard-submitted-label` | Success confirmation |

## How to use this layout in your flows

```yaml
# Drive the home counter
- tapOn: { id: "tab-home" }
- assertVisible: { id: "home-counter-label" }
- tapOn: { id: "home-increment-btn" }
- tapOn: { id: "home-increment-btn" }
- assertVisible:
    id: "home-counter-label"
    text: "2"          # value-aware (text + id combined)

# Drill deep nav
- tapOn: { id: "tab-deepnav" }
- tapOn: { id: "deepnav-l1-movies-btn" }
- tapOn: { id: "deepnav-l2-action-btn" }
- tapOn: { id: "deepnav-l3-item3-btn" }
- tapOn: { id: "deepnav-l4-edit-btn" }
- tapOn: { id: "deepnav-l5-save-btn" }
- assertVisible: { id: "deepnav-l1-saved-label" }   # popped back

# Exercise the wizard
- tapOn: { id: "tab-wizard" }
- tapOn: { id: "wizard-radio-business" }
- tapOn: { id: "wizard-next-btn" }
- assertVisible: { id: "wizard-company-input" }    # conditional field
```

## See also

- [03-selectors.md](03-selectors.md) — selector forms including `id:`
- [08-cookbook.md](08-cookbook.md) — recipes that use these testTag conventions
