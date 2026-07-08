# smix-ffi

UniFFI v0.29+ scaffolding crate for the smix cross-language SDK suite.

Each async endpoint exposes a sibling `cancel_{op}(handle)` for
cancellation (UniFFI does not internally support coroutine cancellation).

## Build pipeline

```bash
# Workspace dev test (macOS host)
cargo build -p smix-ffi
cargo test  -p smix-ffi

# iOS sim slice
cargo build -p smix-ffi --target aarch64-apple-ios-sim --release

# Android sim slice
cargo ndk -t arm64-v8a -t x86_64 build --release

# Swift bindings generation
uniffi-bindgen-swift target/.../libsmix_ffi.a src/smix.udl --out-dir bindings/swift

# Kotlin bindings generation
uniffi-bindgen generate src/smix.udl --language kotlin --out-dir bindings/kotlin

# XCFramework packaging
xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios-sim/release/libsmix_ffi.a -headers include \
  -output dist/SmixCoreFFI.xcframework
```

## Architecture

Wraps stone API (`smix-selector` + `smix-selector-resolver` + `smix-screen`)
into UDL-defined FFI surface. UniFFI generates:
- Rust scaffolding (C-ABI extern "C" fns + RustCallStatus + RustBuffer marshalling)
- Swift bindings (typed Swift API with `async throws` + `Sendable`)
- Kotlin bindings (typed Kotlin API with `suspend fun` + sealed class errors)

This FFI ships only to SDK consumers (XCUITest target / instrumentation test
target / Expo dev-client), **NEVER linked into app binary**. Release app
build has 0 `smix-ffi` bytes.

## License

Apache-2.0 OR MIT (workspace inherited).
