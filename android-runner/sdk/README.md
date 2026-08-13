# smix-android-sdk (`dev.smix.sdk`)

Playwright-style Android emulator UI automation SDK for Kotlin, packaged
as an Android library module. Brings smix's Rust-core selector resolver
to Kotlin via UniFFI 0.29 bindings + a lazy lambda injection pattern.

## Installation

```kotlin
// android-runner/app/build.gradle.kts (consumer side)
dependencies {
    androidTestImplementation("jp.golia.smix:smix-sdk:5.0.0")
}
```

Maven `groupId` is `jp.golia.smix` (reverse DNS of the `smix.golia.jp`
subdomain of GOLIA K.K.). The Kotlin import qualifier `dev.smix.sdk` is
the AAR's internal package namespace per the gradle `android.namespace`
config — intentional mismatch to keep the import stable across coordinate
changes.

This module is intended for test targets only
(`androidTestImplementation` / `debugImplementation` gate) — never
bundled into a production app release variant via gradle build variant +
`consumer-rules.pro`.

## Quick start

The SDK talks to a running smix runner over HTTP — bring one up first
(`smix runner up <serial> --platform android`), then:

```kotlin
import dev.smix.sdk.*
import kotlin.time.Duration.Companion.seconds

class LoginFlowTest {
    @Test
    fun testLoginFlow() = runBlocking {
        val app = Smix.launchApp(AppTarget.BundleId("com.example.MyApp"))

        app.tap(Selector.Id("btn-login"))
        app.fill(Selector.Id("input-username"), "alice")

        val welcome = app.find(Selector.Text(Pattern.Literal("Welcome back")))
        welcome.toBeVisible(timeout = 5.seconds)
    }
}
```

`launchApp` also takes `port: UShort = Smix.defaultRunnerPort` when the
runner is not on the default, and `resolver` / `labelsResolver` for
tests that want to substitute the FFI selector resolver.

## API surface

### Selector

```kotlin
// Base discriminators (untagged JSON wire shape — matches Rust smix-selector)
Selector.Id("btn-login")
Selector.Text(Pattern.Literal("Sign In"))
Selector.Text(Pattern.Regex("^Sub", flags = "i"))
Selector.Label("Settings")
Selector.Role("button")
Selector.Role("button", name = Pattern.Literal("Submit"))
Selector.Focused
Selector.Anchor(AnchorBox(below = Selector.Text(Pattern.Literal("Total"))), IndexModifiers(nth = 0))
Selector.LocalizedText(mapOf("en" to "Submit", "ja" to "送信"))

// Fluent modifier chaining
Selector.Id("btn").below(Selector.Text(Pattern.Literal("Address")))
Selector.Label("Edit").nth(0)
Selector.Text(Pattern.Literal("Item")).above(Selector.Text(Pattern.Literal("Footer"))).first()
Selector.Role("button").near(Selector.Text(Pattern.Literal("Confirm"))).ancestor(Selector.Role("dialog"))
```

Wire JSON (untagged + flatten — byte-identical to Rust smix-selector):

```json
{"id": "btn-login", "below": {"text": "Address"}, "nth": 0}
```

### App (act)

```kotlin
app.tap(selector, timeout = 5.seconds)
app.fill(selector, "alice")
app.pressKey(KeyName.RETURN)      // RETURN / DELETE / SPACE / TAB / ESCAPE
app.swipe(SwipeDirection.UP)      // UP / DOWN / LEFT / RIGHT
app.tapAtCoord(0.5, 0.8)          // 0..1, throws if out-of-range
app.terminate()
app.relaunch()
```

### App (sense)

```kotlin
val tree: A11yNode = app.tree()
val popups: List<SystemPopup> = app.systemPopups()
```

Screenshots, deep links and fresh-launch state wiping are CLI and
YAML-level capabilities (`smix screenshot`, `openLink:`, `launchApp:
{ clearState: true }`); the Kotlin surface is sense + act against a
session the runner already holds.

### Locator (assertions)

```kotlin
val loc = app.find(Selector.Text(Pattern.Literal("Welcome")))
loc.toBeVisible(timeout = 5.seconds)    // polls 250ms
loc.toContainText("alice", timeout = 5.seconds)
loc.toHaveLabel("Sign In", timeout = 5.seconds)
loc.toHaveCount(3, timeout = 5.seconds)
```

### ExpectationFailure (AI-readable JSON)

```kotlin
try {
    app.tap(Selector.Id("btn-missing"))
} catch (e: ExpectationFailure) {
    e.code              // ELEMENT_NOT_FOUND / NOT_VISIBLE / NOT_ENABLED / AMBIGUOUS / TIMEOUT / ASSERTION_FAILED / APP_NOT_RUNNING / SIMULATOR_NOT_BOOTED / DRIVER_ERROR
    e.message           // human-readable
    e.selectorJson      // original Selector encoded as JSON
    e.visibleElements   // List<A11yNode> — first 20 nodes from current tree
    e.suggestions       // List<String> — ["check accessibilityIdentifier...", ...]
    e.errorJson()       // single-line sorted-keys JSON for AI agent consumption
}
```

## Architecture: lazy lambda capture injection

The Kotlin SDK uses **SelectorResolver injection** (the default wraps
`uniffi.smix.resolveSelector` behind a lazy lambda) so a JVM unit test
can substitute its own resolver without triggering JNA initialization.
`libuniffi_smix.so` is Android-arch and unloadable on the host JVM;
deferring the class load to `.resolve()` keeps the default JNA-safe for
JVM tests, which never invoke it.

`SelectorResolver` is a `fun interface` — one method, so a lambda is a
complete implementation. No mock types ship with the SDK; supply your
own.

```kotlin
// Production path (Android instrumentation) — resolver defaults to
// DefaultFfiResolver, which loads uniffi.smix.SmixKt via JNA:
val app = Smix.launchApp(AppTarget.BundleId("com.example.MyApp"))

// Test path (JVM unit test) — a lambda stands in, so the FFI default
// is never invoked and the .so is never loaded:
val app = Smix.launchApp(
    AppTarget.BundleId("com.example.MyApp"),
    resolver = { _, _ -> listOf("btn-login") },
    labelsResolver = { _, _ -> listOf("Sign In") },
)
```

## Conformance

Cross-binary harness verifies SDK output byte-identical to Rust:

```bash
bash ../../scripts/sdk/run-cross-binary-harness.sh
# Summary: 24 / 24 fixtures byte-identical (Rust + Swift + TS)
```

## License

Apache-2.0 OR MIT (dual, at your option).
