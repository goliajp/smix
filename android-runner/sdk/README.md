# smix-android-sdk (`dev.smix.sdk`)

Playwright-style Android emulator UI automation SDK for Kotlin, packaged
as an Android library module. Brings smix's Rust-core selector resolver
to Kotlin via UniFFI 0.29 bindings + a lazy lambda injection pattern.

## Installation

```kotlin
// android-runner/app/build.gradle.kts (consumer side)
dependencies {
    androidTestImplementation("jp.golia.smix:smix-sdk:2.0.0")
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

## Quick start (MockSimRuntime — JVM unit tests)

```kotlin
import dev.smix.sdk.*

class LoginFlowTest {
    @Test
    fun testLoginFlow() = runBlocking {
        val runtime = MockSimRuntime(snapshotResult = /* A11yNode tree */)
        val resolver = MockSelectorResolver().apply {
            registerHit("""{"id":"btn-login"}""", "btn-login")
        }
        val labels = MockLabelsResolver()

        val app = Smix.launchApp(
            AppTarget.BundleId("com.example.MyApp"),
            runtime,
            resolver,
            labels,
        )

        app.tap(Selector.Id("btn-login"))
        app.fill(Selector.Id("input-username"), "alice")

        val welcome = app.find(Selector.Text(Pattern.Literal("Welcome back")))
        welcome.toBeVisible(timeout = 5.seconds)
    }
}
```

## Production: UiAutomator-backed runtime

A real instrumentation backend wraps UiAutomator2 + the smix-android-runner
HTTP server. Test authors either provide a concrete `SmixSimRuntime` impl
that talks to that HTTP surface, or use `MockSimRuntime` for JVM unit
testing.

```kotlin
val runtime = HttpSimRuntime(/* runner endpoint */)
val app = Smix.launchApp(AppTarget.BundleId("com.example.MyApp"), runtime)
// ... rest identical to Mock-based example
```

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
app.tap(selector: Selector, timeout: Duration = 5.seconds)
app.fill(selector: Selector, text: String)
app.pressKey(key: KeyName)  // RETURN / DELETE / SPACE / TAB / ESCAPE / ENTER
app.swipe(direction: SwipeDirection)  // UP / DOWN / LEFT / RIGHT
app.tapAtCoord(nx: Double, ny: Double)  // 0..1, throws if out-of-range
app.terminate()
app.relaunch()
app.launchFresh(clearState = true, clearKeychain = true)
```

### App (sense)

```kotlin
val png: ByteArray = app.screenshot()
val tree: A11yNode = app.tree()
val popups: List<A11yNode> = app.systemPopups()
app.openUrl("myapp://deep/link")
```

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

The Kotlin SDK uses **SelectorResolver injection** (default lazy-lambda
wraps `uniffi.smix.resolveSelector`) so JVM unit tests can pass
`MockSelectorResolver` without triggering JNA initialization.
`libuniffi_smix.so` is Android-arch and unloadable on the host JVM;
lazy lambda capture defers `SmixKt` class load to `.resolve()` invocation,
which is JNA-safe for JVM tests since tests never invoke the default.

```kotlin
// Production path (Android instrumentation):
val app = Smix.launchApp(AppTarget.BundleId("..."), runtime)
// Uses DefaultFfiResolver -> loads uniffi.smix.SmixKt -> JNA loads libuniffi_smix.so

// Test path (JVM unit test):
val app = Smix.launchApp(AppTarget.BundleId("..."), runtime, MockSelectorResolver(), MockLabelsResolver())
// Mock provided -> DefaultFfiResolver never invoked -> SmixKt never loaded -> no JNA needed
```

## Conformance

Cross-binary harness verifies SDK output byte-identical to Rust:

```bash
bash ../../scripts/sdk/run-cross-binary-harness.sh
# Summary: 24 / 24 fixtures byte-identical (Rust + Swift + TS)
```

## License

Apache-2.0 OR MIT (dual, at your option).
