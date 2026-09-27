fn main() {
    uniffi::generate_scaffolding("./src/smix.udl").expect("UniFFI scaffolding generation failed");
    // The digest of the sources this library is built from, computed by
    // scripts/sdk/ffi-source-digest.py and passed in by the scripts that
    // build the shipped libraries. A plain `cargo build` has none, and says
    // so in the same place, so an unstamped library cannot pass for one.
    println!("cargo:rerun-if-env-changed=SMIX_FFI_SOURCE_DIGEST");
    let digest = std::env::var("SMIX_FFI_SOURCE_DIGEST").unwrap_or_else(|_| "unstamped".into());
    println!("cargo:rustc-env=SMIX_FFI_SOURCE_STAMP=smix-ffi-source:{digest}");
}
