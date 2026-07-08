# smix-core-conformance

Cross-SDK conformance test harness for the smix native SDK suite.

## Purpose

Single fixture set drives byte-identical validation across:
- Rust core (this crate's binary + unit tests)
- Swift SDK (via SmixSDK target in `swift-bridge/Tests/SmixSDKTests/`)
- Kotlin SDK (via instrumentation test in `android-runner/sdk/...`)
- Expo RN bridge (via jest in `goliajp/smix-sdk-rn/test/`)

Any backend producing different bytes for the same fixture =
conformance fail = SDK release blocked.

## Fixture format

`fixtures/<id>.json`:

```json
{
  "id": "<unique fixture id>",
  "description": "<one-line>",
  "tree": <A11yNode JSON>,
  "selector": <Selector JSON>,
  "expected": <Vec<String> of matched node ids>
}
```

## Running

```bash
# Rust backend
cargo run --bin fixture-runner -- rust spike-001

# Unit test (Rust backend, all fixtures)
cargo test -p smix-core-conformance

# Cross-SDK comparison
diff <(cargo run --bin fixture-runner -- rust spike-001) \
     <(swift run --package-path swift-bridge SwiftFixtureRunner -- spike-001)
```

## License

Apache-2.0 OR MIT (workspace inherited).
