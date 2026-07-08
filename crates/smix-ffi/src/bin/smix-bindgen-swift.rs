//! smix-bindgen-swift — thin wrapper around `uniffi::uniffi_bindgen_swift()`
//! (UniFFI 0.29.5+ built-in CLI).
//!
//! Avoids needing a separate `cargo install uniffi-bindgen-swift` since
//! `uniffi` v0.29+ ships the Swift generator inside the main crate, gated
//! behind the `cli` feature (which our `bindgen-cli` feature enables).
//!
//! Usage:
//!   cargo run -p smix-ffi --features bindgen-cli --bin smix-bindgen-swift -- \
//!     target/aarch64-apple-darwin/release/libsmix_ffi.dylib \
//!     swift-bridge/Sources/SmixCoreFFI/Generated \
//!     --xcframework --module-name SmixCoreFFI \
//!     --modulemap-filename module.modulemap

fn main() {
    uniffi::uniffi_bindgen_swift();
}
