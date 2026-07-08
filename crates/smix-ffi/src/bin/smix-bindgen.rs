//! smix-bindgen — multi-language bindings generator (UniFFI 0.29.5 built-in).
//!
//! Wraps `uniffi::uniffi_bindgen_main()` — supports `--language kotlin /
//! python / ruby` etc. v7.0 c4 uses it for Kotlin bindings; Swift goes
//! through the dedicated `smix-bindgen-swift` bin (different CLI shape).
//!
//! Usage:
//!   cargo run -p smix-ffi --features bindgen-cli --bin smix-bindgen -- \
//!     generate crates/smix-ffi/src/smix.udl --language kotlin --out-dir <dir>

fn main() {
    uniffi::uniffi_bindgen_main();
}
