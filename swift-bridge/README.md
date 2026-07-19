# SmixSDK (Swift)

Playwright-style iOS Simulator automation SDK for Swift. Brings smix's
Rust-core selector resolver + sense/act pipeline to native Swift via UniFFI
bindings, packaged as a Swift Package Manager binary target.

## Installation

```swift
// Package.swift
dependencies: [
    .package(url: "https://github.com/goliajp/smix", from: "0.1.0"),
]

targets: [
    .target(name: "MyAppUITests", dependencies: [
        .product(name: "SmixSDK", package: "smix"),
    ]),
]
```

## Quick start

```swift
import XCTest
import SmixSDK

final class LoginFlowTests: XCTestCase {
    func testLoginFlow() async throws {
        // The SDK ships with MockSimRuntime (deterministic in-memory tree,
        // used by SmixSDKTests in this repo). For real-sim runs, provide a
        // concrete `SmixSimRuntime` impl (e.g. an XCUITest-backed runtime
        // wrapping XCUIApplication + IOHID synthesis via SmixRunnerCore).
        let runtime = MockSimRuntime(snapshotResult: /* ... */)
        let app = try await Smix.launchApp(
            .bundleId("com.example.MyApp"),
            runtime: runtime,
        )

        try await app.tap(.id("btn-login"))
        try await app.fill(.id("input-username"), "alice")
        try await app.fill(.id("input-password"), "secret")
        try await app.tap(.label("Sign In"))

        let welcome = app.find(.text("Welcome back"))
        try await welcome.toBeVisible(timeout: .seconds(5))
    }
}
```

## API surface

### Selector

```swift
// Base discriminators (untagged JSON wire shape — matches Rust smix-selector)
.id("btn-login")
.text("Sign In")
.text(.regex("^Sub", flags: "i"))
.label("Settings")
.role(.button)
.role(.button, name: "Submit")
.focused
.anchor(AnchorBox(below: .text("Total")), index: IndexModifiers(nth: 0))
.localizedText(["en": "Submit", "ja": "送信"])

// Fluent modifier chaining (returns new Selector with Modifiers updated)
.id("btn").below(.text("Address"))
.label("Edit").nth(0)
.text("Item").above(.text("Footer")).first()
.role(.button).near(.text("Confirm")).ancestor(.role(.dialog))
```

Wire JSON (untagged + flatten — byte-identical to Rust smix-selector):

```json
{"id": "btn-login", "below": {"text": "Address"}, "nth": 0}
```

### App (act)

```swift
try await app.tap(_ selector: Selector, timeout: Duration = .seconds(5))
try await app.fill(_ selector: Selector, _ text: String)
try await app.pressKey(_ key: KeyName)  // .return / .delete / .space / .tab / .escape / .enter
try await app.swipe(_ direction: SwipeDirection)  // .up / .down / .left / .right
try await app.tapAtCoord(_ nx: Double, _ ny: Double)  // 0..1, throws if out-of-range
try await app.terminate()
try await app.relaunch()
try await app.launchFresh(clearState: true, clearKeychain: true)
```

### App (sense)

```swift
let png: Data = try await app.screenshot()
let tree: A11yNode = try await app.tree()
let popups: [A11yNode] = try await app.systemPopups()
try await app.openUrl(URL(string: "myapp://deep/link")!)
```

### Locator (assertions)

```swift
let loc = app.find(.text("Welcome"))
try await loc.toBeVisible(timeout: .seconds(5))    // polls 250ms
try await loc.toContainText("alice")               // substring in label/title/text
try await loc.toHaveLabel("Sign In")               // strict equal
try await loc.toHaveCount(3)                       // exact match count
```

### ExpectationFailure (AI-readable JSON)

```swift
do {
    try await app.tap(.id("btn-missing"))
} catch let failure as ExpectationFailure {
    failure.code              // .elementNotFound / .notVisible / .notEnabled / .ambiguous / .timeout / .assertionFailed / .appNotRunning / .simulatorNotBooted / .driverError
    failure.message           // human-readable
    failure.selector          // original Selector (preserves chain)
    failure.visibleElements   // [A11yNode] — first 20 nodes from current tree
    failure.suggestions       // ["check `accessibilityIdentifier` is set", ...]
    failure.errorDescription  // single-line sorted-keys JSON for AI agent consumption
}
```

## Conformance

Every fixture in `crates/smix-core-conformance/fixtures/` passes through
Rust, Swift, and TypeScript backends with byte-identical output. Run the
cross-binary harness:

```bash
bash scripts/sdk/run-cross-binary-harness.sh
# Summary: 24 / 24 fixtures byte-identical (Rust + Swift + TS)
```

This validates that the UniFFI binding generator preserves wire shape AND
that the Swift `JSONSerialization` re-encode is equivalent to Rust `serde`
encode — i.e. the SDK's wire layer is correct, not just the high-level
API.

## Architecture

- **Rust stone core** (`crates/smix-{selector,selector-resolver,screen,error}`):
  selector resolution + AI-readable failure types, shared across all
  language SDKs.
- **UniFFI scaffolding** (`crates/smix-ffi`): `#[no_mangle] extern "C"`
  wrappers + auto-generated Swift/Kotlin bindings.
- **Swift SDK** (`Sources/SmixSDK/`): Playwright-style facade —
  `Smix` / `App` / `Selector` / `Modifiers` / `Locator` /
  `ExpectationFailure`. Type-safe enum API; fluent modifier chaining.
- **Runtime abstraction** (`SmixSimRuntime` protocol): pluggable
  backend for simulator I/O (launch / snapshot / tap / synthesize).
  Test code uses `MockSimRuntime` (in-memory, deterministic); real-sim
  runs plug in an XCUITest-backed runtime.

## Test-target-only

SmixSDK is intended for XCUITest / SwiftPM `testTarget` use only —
never linked into a production app binary. The Swift Package binary
target only includes simulator slices (`ios-arm64-simulator`,
`macos-arm64`) — no `ios-arm64` device slice.

## Release pipeline

`.github/workflows/smix-sdk-swift-release.yml` — triggered by
`git push --tags swift-v*` OR `workflow_dispatch`. Builds XCFramework,
runs full test suite + cross-binary harness as CI gate, packages
into zip + sha256, creates GitHub Release with Swift Package
binaryTarget consumer snippet.
